//! Pure helpers (no Windows calls), unit tested.

use arui_win::{NetworkKind, NetworkState};

/// IANA ifType values we care about.
const IANA_ETHERNET: u32 = 6;
const IANA_WIFI: u32 = 71;
const IANA_WWAN_PP: u32 = 243;
const IANA_WWAN_PP2: u32 = 244;

/// `NetworkConnectivityLevel` raw values.
pub const LEVEL_NONE: i32 = 0;
pub const LEVEL_LOCAL: i32 = 1;
#[allow(dead_code)]
pub const LEVEL_CONSTRAINED: i32 = 2;
pub const LEVEL_INTERNET: i32 = 3;

/// What we read from the active `ConnectionProfile`, as plain data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawNetwork {
    /// Whether any connected profile was found.
    pub has_profile: bool,
    /// Raw `NetworkConnectivityLevel`.
    pub level: i32,
    pub is_wlan: bool,
    pub is_wwan: bool,
    /// `NetworkAdapter.IanaInterfaceType`, if the profile has an adapter.
    pub iana_type: Option<u32>,
    pub name: Option<String>,
}

pub fn derive_network(raw: &RawNetwork) -> NetworkState {
    let connected = raw.has_profile && raw.level >= LEVEL_LOCAL;
    if !connected {
        return NetworkState { connected: false, internet: false, kind: NetworkKind::None, name: None };
    }
    let kind = if raw.is_wlan {
        NetworkKind::Wifi
    } else if raw.is_wwan {
        NetworkKind::Cellular
    } else {
        match raw.iana_type {
            Some(IANA_ETHERNET) => NetworkKind::Ethernet,
            Some(IANA_WIFI) => NetworkKind::Wifi,
            Some(IANA_WWAN_PP) | Some(IANA_WWAN_PP2) => NetworkKind::Cellular,
            _ => NetworkKind::Other,
        }
    };
    NetworkState {
        connected,
        internet: raw.level == LEVEL_INTERNET,
        kind,
        name: raw.name.clone().filter(|n| !n.is_empty()),
    }
}

/// Clamps a requested volume level to 0..=1 (NaN becomes 0).
pub fn clamp_level(level: f32) -> f32 {
    if level.is_nan() {
        0.0
    } else {
        level.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(level: i32) -> RawNetwork {
        RawNetwork { has_profile: true, level, ..Default::default() }
    }

    #[test]
    fn disconnected() {
        let s = derive_network(&RawNetwork::default());
        assert_eq!(s, NetworkState { connected: false, internet: false, kind: NetworkKind::None, name: None });
        // Profile without connectivity also counts as disconnected and hides the name.
        let s = derive_network(&RawNetwork { level: LEVEL_NONE, name: Some("x".into()), ..raw(0) });
        assert!(!s.connected);
        assert_eq!(s.name, None);
    }

    #[test]
    fn wifi_with_internet() {
        let s = derive_network(&RawNetwork {
            is_wlan: true,
            iana_type: Some(IANA_WIFI),
            name: Some("HomeNet".into()),
            ..raw(LEVEL_INTERNET)
        });
        assert_eq!(
            s,
            NetworkState { connected: true, internet: true, kind: NetworkKind::Wifi, name: Some("HomeNet".into()) }
        );
    }

    #[test]
    fn ethernet_local_only() {
        let s = derive_network(&RawNetwork { iana_type: Some(IANA_ETHERNET), ..raw(LEVEL_LOCAL) });
        assert!(s.connected);
        assert!(!s.internet);
        assert_eq!(s.kind, NetworkKind::Ethernet);
    }

    #[test]
    fn constrained_is_not_internet() {
        let s = derive_network(&RawNetwork { iana_type: Some(IANA_ETHERNET), ..raw(LEVEL_CONSTRAINED) });
        assert!(s.connected && !s.internet);
    }

    #[test]
    fn kinds() {
        assert_eq!(derive_network(&RawNetwork { is_wwan: true, ..raw(3) }).kind, NetworkKind::Cellular);
        assert_eq!(derive_network(&RawNetwork { iana_type: Some(243), ..raw(3) }).kind, NetworkKind::Cellular);
        assert_eq!(derive_network(&RawNetwork { iana_type: Some(131), ..raw(3) }).kind, NetworkKind::Other);
        assert_eq!(derive_network(&raw(3)).kind, NetworkKind::Other);
        assert_eq!(derive_network(&RawNetwork { name: Some(String::new()), ..raw(3) }).name, None);
    }

    #[test]
    fn clamping() {
        assert_eq!(clamp_level(-0.5), 0.0);
        assert_eq!(clamp_level(0.25), 0.25);
        assert_eq!(clamp_level(7.0), 1.0);
        assert_eq!(clamp_level(f32::NAN), 0.0);
        assert_eq!(clamp_level(f32::INFINITY), 1.0);
    }
}
