//! Pure byte-layout parsing of the data `Shell_NotifyIconW` and `Shell_NotifyIconGetRect` send to
//! the `Shell_TrayWnd` window through WM_COPYDATA. No Win32 calls here, so it is unit testable.
//!
//! Layouts (all little endian, every handle is stored as a 32-bit value even on 64-bit Windows):
//!
//! dwData == 1, SHELLTRAYDATA:
//! ```text
//! 0   u32  dwSignature (0x34753423)
//! 4   u32  dwMessage   (NIM_*)
//! 8   NOTIFYICONDATA32:
//!     +0   u32  cbSize
//!     +4   u32  hWnd
//!     +8   u32  uID
//!     +12  u32  uFlags
//!     +16  u32  uCallbackMessage
//!     +20  u32  hIcon
//!     +24  wchar szTip[64]  (V1, cbSize <= ~168) or szTip[128] (V2 and later)
//!     --- only in V2 and later:
//!     +280 u32  dwState
//!     +284 u32  dwStateMask
//!     +288 wchar szInfo[256]
//!     +800 u32  uVersion / uTimeout (union)
//!     +804 wchar szInfoTitle[64]
//!     +932 u32  dwInfoFlags
//!     --- V3 and later:
//!     +936 GUID guidItem
//!     --- V4 (Vista) and later:
//!     +952 u32  hBalloonIcon
//!     = 956 bytes
//! ```
//!
//! dwData == 3, WINNOTIFYICONIDENTIFIER (Shell_NotifyIconGetRect):
//! ```text
//! 0  u32 dwMagic, 4 u32 dwMessage (1 = top-left, 2 = bottom-right), 8 u32 cbSize,
//! 12 u32 padding, 16 u32 hWnd, 20 u32 uID, 24 GUID guidItem (40 bytes total)
//! ```
//!
//! The parser bounds every field by both the buffer length and `cbSize`, so older or truncated
//! structures read missing fields as zero / empty instead of failing.

pub const SHELLTRAY_SIGNATURE: u32 = 0x3475_3423;

pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIM_SETFOCUS: u32 = 3;
pub const NIM_SETVERSION: u32 = 4;

pub const NIF_MESSAGE: u32 = 0x01;
pub const NIF_ICON: u32 = 0x02;
pub const NIF_TIP: u32 = 0x04;
pub const NIF_STATE: u32 = 0x08;
pub const NIF_INFO: u32 = 0x10;
pub const NIF_GUID: u32 = 0x20;
pub const NIF_REALTIME: u32 = 0x40;
pub const NIF_SHOWTIP: u32 = 0x80;

pub const NIS_HIDDEN: u32 = 0x01;
pub const NIS_SHAREDICON: u32 = 0x02;

/// Size of the 32-bit NOTIFYICONDATAW with every field (Vista+).
pub const NID32_SIZE: usize = 956;
/// Size of the original (Windows 95/NT4) 32-bit NOTIFYICONDATAW: 6 dwords + szTip[64].
pub const NID32_V1_SIZE: usize = 152;
/// cbSize values at or below this are treated as V1 (64 char tooltip, nothing after it).
/// Covers the 32-bit (152) and 64-bit (168) V1 sizes; every later size is above 900.
const V1_CBSIZE_MAX: u32 = 200;

const NID_OFFSET: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    TooShort { len: usize, need: usize },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::TooShort { len, need } => {
                write!(f, "buffer too short: {len} bytes, need at least {need}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// The fields of a NOTIFYICONDATA that ARUI cares about, already decoded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotifyIconData {
    pub cb_size: u32,
    /// Owner window, zero-extended from the 32-bit value (win32k only looks at the low 32 bits).
    pub hwnd: u64,
    pub uid: u32,
    pub flags: u32,
    pub callback_message: u32,
    /// Icon handle, zero-extended from the 32-bit value.
    pub hicon: u64,
    pub tip: String,
    pub state: u32,
    pub state_mask: u32,
    pub info: String,
    /// `uVersion` for NIM_SETVERSION, `uTimeout` otherwise (same storage).
    pub version_or_timeout: u32,
    pub info_title: String,
    pub info_flags: u32,
    /// Formatted `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`; `None` when absent or all zero.
    pub guid: Option<String>,
    pub balloon_icon: u64,
    /// True when the structure was the V1 layout (64 char tooltip).
    pub is_v1: bool,
}

impl NotifyIconData {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    /// The GUID that identifies this icon, only when NIF_GUID says to use it.
    pub fn identity_guid(&self) -> Option<&str> {
        if self.has(NIF_GUID) {
            self.guid.as_deref()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMessage {
    pub signature: u32,
    /// NIM_* value.
    pub message: u32,
    pub nid: NotifyIconData,
}

/// Request sent by `Shell_NotifyIconGetRect` (WM_COPYDATA dwData == 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconIdentifier {
    pub magic: u32,
    /// 1 = asking for the top-left corner, 2 = asking for the bottom-right corner.
    pub message: u32,
    pub cb_size: u32,
    pub hwnd: u64,
    pub uid: u32,
    pub guid: Option<String>,
}

/// Bounded little-endian reader over a byte slice.
struct Reader<'a> {
    buf: &'a [u8],
    /// Fields that end past this offset read as zero / empty.
    limit: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8], limit: usize) -> Self {
        Self {
            buf,
            limit: limit.min(buf.len()),
        }
    }

    fn bytes(&self, off: usize, len: usize) -> Option<&'a [u8]> {
        if off + len <= self.limit {
            Some(&self.buf[off..off + len])
        } else {
            None
        }
    }

    fn u32(&self, off: usize) -> u32 {
        self.bytes(off, 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(0)
    }

    /// Reads a fixed `chars`-long UTF-16 field, stopping at the first NUL. A field that is only
    /// partly inside the limit is read up to the limit.
    fn wstr(&self, off: usize, chars: usize) -> String {
        if off >= self.limit {
            return String::new();
        }
        let avail = ((self.limit - off) / 2).min(chars);
        let units: Vec<u16> = (0..avail)
            .map(|i| {
                let p = off + i * 2;
                u16::from_le_bytes([self.buf[p], self.buf[p + 1]])
            })
            .take_while(|&c| c != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }

    fn guid(&self, off: usize) -> Option<String> {
        self.bytes(off, 16).and_then(format_guid)
    }
}

/// Formats 16 raw GUID bytes (Windows in-memory order) as `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
/// Returns `None` for the all-zero GUID.
pub fn format_guid(b: &[u8]) -> Option<String> {
    if b.len() < 16 || b[..16].iter().all(|&x| x == 0) {
        return None;
    }
    let d1 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let d2 = u16::from_le_bytes([b[4], b[5]]);
    let d3 = u16::from_le_bytes([b[6], b[7]]);
    Some(format!(
        "{{{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    ))
}

/// Parses a NOTIFYICONDATA32 that starts at the beginning of `buf`.
pub fn parse_notify_icon_data(buf: &[u8]) -> Result<NotifyIconData, ParseError> {
    // The six leading dwords are present in every version.
    const MIN: usize = 24;
    if buf.len() < MIN {
        return Err(ParseError::TooShort {
            len: buf.len(),
            need: MIN,
        });
    }
    let head = Reader::new(buf, buf.len());
    let cb_size = head.u32(0);
    let is_v1 = cb_size != 0 && cb_size <= V1_CBSIZE_MAX;
    // cbSize may be the 64-bit struct size (bigger than ours), so it only ever shrinks the limit.
    // A zero cbSize is nonsense; fall back to whatever the buffer holds.
    let limit = if is_v1 {
        NID32_V1_SIZE
    } else if cb_size == 0 {
        buf.len()
    } else {
        (cb_size as usize).max(NID32_V1_SIZE)
    };
    let r = Reader::new(buf, limit);

    let mut nid = NotifyIconData {
        cb_size,
        hwnd: r.u32(4) as u64,
        uid: r.u32(8),
        flags: r.u32(12),
        callback_message: r.u32(16),
        hicon: r.u32(20) as u64,
        is_v1,
        ..Default::default()
    };
    if is_v1 {
        nid.tip = r.wstr(24, 64);
        return Ok(nid);
    }
    nid.tip = r.wstr(24, 128);
    nid.state = r.u32(280);
    nid.state_mask = r.u32(284);
    nid.info = r.wstr(288, 256);
    nid.version_or_timeout = r.u32(800);
    nid.info_title = r.wstr(804, 64);
    nid.info_flags = r.u32(932);
    nid.guid = r.guid(936);
    nid.balloon_icon = r.u32(952) as u64;
    Ok(nid)
}

/// Parses the payload of WM_COPYDATA with dwData == 1.
pub fn parse_tray_data(buf: &[u8]) -> Result<TrayMessage, ParseError> {
    if buf.len() < NID_OFFSET {
        return Err(ParseError::TooShort {
            len: buf.len(),
            need: NID_OFFSET,
        });
    }
    let r = Reader::new(buf, buf.len());
    Ok(TrayMessage {
        signature: r.u32(0),
        message: r.u32(4),
        nid: parse_notify_icon_data(&buf[NID_OFFSET..])?,
    })
}

/// Parses the payload of WM_COPYDATA with dwData == 3.
pub fn parse_icon_identifier(buf: &[u8]) -> Result<IconIdentifier, ParseError> {
    const MIN: usize = 24;
    if buf.len() < MIN {
        return Err(ParseError::TooShort {
            len: buf.len(),
            need: MIN,
        });
    }
    let r = Reader::new(buf, buf.len());
    Ok(IconIdentifier {
        magic: r.u32(0),
        message: r.u32(4),
        cb_size: r.u32(8),
        hwnd: r.u32(16) as u64,
        uid: r.u32(20),
        guid: r.guid(24),
    })
}

/// Packs two signed 16-bit coordinates the way MAKELPARAM / MAKEWPARAM do.
pub fn make_long(lo: i32, hi: i32) -> u32 {
    (lo as u16 as u32) | ((hi as u16 as u32) << 16)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test helper that builds NOTIFYICONDATA32 / SHELLTRAYDATA byte buffers by hand.
    pub struct NidBuilder {
        pub buf: Vec<u8>,
    }

    impl NidBuilder {
        pub fn new(size: usize) -> Self {
            let mut b = Self { buf: vec![0; size] };
            b.u32(0, size as u32);
            b
        }
        pub fn u32(&mut self, off: usize, v: u32) -> &mut Self {
            self.buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
            self
        }
        pub fn wstr(&mut self, off: usize, s: &str) -> &mut Self {
            for (i, c) in s.encode_utf16().enumerate() {
                self.buf[off + i * 2..off + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
            }
            self
        }
        pub fn bytes(&mut self, off: usize, b: &[u8]) -> &mut Self {
            self.buf[off..off + b.len()].copy_from_slice(b);
            self
        }
        pub fn ids(&mut self, hwnd: u32, uid: u32, flags: u32, cb: u32, hicon: u32) -> &mut Self {
            self.u32(4, hwnd)
                .u32(8, uid)
                .u32(12, flags)
                .u32(16, cb)
                .u32(20, hicon)
        }
        /// Wraps the NOTIFYICONDATA in a SHELLTRAYDATA header.
        pub fn shelltray(&self, message: u32) -> Vec<u8> {
            let mut out = Vec::with_capacity(self.buf.len() + 8);
            out.extend_from_slice(&SHELLTRAY_SIGNATURE.to_le_bytes());
            out.extend_from_slice(&message.to_le_bytes());
            out.extend_from_slice(&self.buf);
            out
        }
    }

    /// {6F3C9D61-12AB-4C5E-8F00-0123456789AB} in Windows in-memory order.
    pub const GUID_BYTES: [u8; 16] = [
        0x61, 0x9D, 0x3C, 0x6F, 0xAB, 0x12, 0x5E, 0x4C, 0x8F, 0x00, 0x01, 0x23, 0x45, 0x67, 0x89,
        0xAB,
    ];
    pub const GUID_STR: &str = "{6F3C9D61-12AB-4C5E-8F00-0123456789AB}";

    #[test]
    fn full_vista_layout() {
        let mut b = NidBuilder::new(NID32_SIZE);
        b.ids(
            0x0001_02A4,
            7,
            NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_STATE | NIF_INFO | NIF_GUID | NIF_SHOWTIP,
            0x8001,
            0x0042_0F11,
        )
        .wstr(24, "Discord - 3 unread")
        .u32(280, NIS_HIDDEN)
        .u32(284, NIS_HIDDEN | NIS_SHAREDICON)
        .wstr(288, "New message from someone")
        .u32(800, 10_000)
        .wstr(804, "Discord")
        .u32(932, 1)
        .bytes(936, &GUID_BYTES)
        .u32(952, 0x1234);
        let msg = parse_tray_data(&b.shelltray(NIM_ADD)).unwrap();
        assert_eq!(msg.signature, SHELLTRAY_SIGNATURE);
        assert_eq!(msg.message, NIM_ADD);
        let n = msg.nid;
        assert!(!n.is_v1);
        assert_eq!(n.cb_size, NID32_SIZE as u32);
        assert_eq!(n.hwnd, 0x0001_02A4);
        assert_eq!(n.uid, 7);
        assert_eq!(n.callback_message, 0x8001);
        assert_eq!(n.hicon, 0x0042_0F11);
        assert_eq!(n.tip, "Discord - 3 unread");
        assert_eq!(n.state, NIS_HIDDEN);
        assert_eq!(n.state_mask, NIS_HIDDEN | NIS_SHAREDICON);
        assert_eq!(n.info, "New message from someone");
        assert_eq!(n.version_or_timeout, 10_000);
        assert_eq!(n.info_title, "Discord");
        assert_eq!(n.info_flags, 1);
        assert_eq!(n.guid.as_deref(), Some(GUID_STR));
        assert_eq!(n.identity_guid(), Some(GUID_STR));
        assert_eq!(n.balloon_icon, 0x1234);
        assert!(n.has(NIF_SHOWTIP));
    }

    #[test]
    fn v1_layout_short_tip_and_no_extras() {
        let mut b = NidBuilder::new(NID32_V1_SIZE);
        b.ids(0x10, 1, NIF_TIP | NIF_ICON, 0x400, 0x20)
            .wstr(24, "Old app");
        // Garbage past the V1 struct must not be read as state / guid.
        let mut data = b.shelltray(NIM_MODIFY);
        data.extend(std::iter::repeat_n(0xFF, NID32_SIZE - NID32_V1_SIZE));
        let n = parse_tray_data(&data).unwrap().nid;
        assert!(n.is_v1);
        assert_eq!(n.tip, "Old app");
        assert_eq!(n.state, 0);
        assert_eq!(n.guid, None);
        assert_eq!(n.info, "");
    }

    #[test]
    fn v1_tip_is_capped_at_64_chars() {
        let mut b = NidBuilder::new(NID32_V1_SIZE);
        let long: String = "x".repeat(64);
        b.wstr(24, &long);
        let n = parse_notify_icon_data(&b.buf).unwrap();
        assert_eq!(n.tip.len(), 64);
    }

    #[test]
    fn v2_size_has_no_guid() {
        // NOTIFYICONDATAW_V2_SIZE (32-bit) ends right before guidItem.
        let mut b = NidBuilder::new(NID32_SIZE);
        b.u32(0, 936)
            .ids(1, 2, NIF_INFO | NIF_GUID, 0, 0)
            .wstr(288, "hi")
            .bytes(936, &GUID_BYTES);
        let n = parse_notify_icon_data(&b.buf).unwrap();
        assert_eq!(n.info, "hi");
        assert_eq!(n.guid, None, "guid lies outside cbSize");
        assert_eq!(n.identity_guid(), None);
    }

    #[test]
    fn x64_cbsize_reads_everything() {
        // 64-bit apps report cbSize = 976 but shell32 still sends the 32-bit layout.
        let mut b = NidBuilder::new(NID32_SIZE);
        b.u32(0, 976)
            .ids(5, 6, NIF_GUID, 0, 0)
            .bytes(936, &GUID_BYTES);
        let n = parse_notify_icon_data(&b.buf).unwrap();
        assert!(!n.is_v1);
        assert_eq!(n.guid.as_deref(), Some(GUID_STR));
    }

    #[test]
    fn guid_without_flag_is_not_identity() {
        let mut b = NidBuilder::new(NID32_SIZE);
        b.ids(5, 6, NIF_TIP, 0, 0).bytes(936, &GUID_BYTES);
        let n = parse_notify_icon_data(&b.buf).unwrap();
        assert!(n.guid.is_some());
        assert_eq!(n.identity_guid(), None);
    }

    #[test]
    fn zero_guid_is_none() {
        assert_eq!(format_guid(&[0; 16]), None);
        assert_eq!(format_guid(&GUID_BYTES).as_deref(), Some(GUID_STR));
    }

    #[test]
    fn truncated_buffer_reads_partial() {
        // A buffer that stops inside szInfo: fields before it decode, the rest are empty.
        let mut b = NidBuilder::new(NID32_SIZE);
        b.ids(1, 2, NIF_TIP | NIF_STATE, 0, 0)
            .wstr(24, "tip")
            .u32(280, 1)
            .u32(284, 1)
            .wstr(288, "abcdef");
        let cut = &b.buf[..288 + 6];
        let n = parse_notify_icon_data(cut).unwrap();
        assert_eq!(n.tip, "tip");
        assert_eq!(n.state, 1);
        assert_eq!(n.info, "abc");
        assert_eq!(n.info_title, "");
        assert_eq!(n.guid, None);
    }

    #[test]
    fn too_short_is_error() {
        assert!(parse_tray_data(&[0; 4]).is_err());
        assert!(parse_tray_data(&[0; 20]).is_err());
        assert!(parse_icon_identifier(&[0; 10]).is_err());
    }

    #[test]
    fn full_length_tip_without_nul() {
        let mut b = NidBuilder::new(NID32_SIZE);
        b.wstr(24, &"y".repeat(128)).u32(280, 0x55);
        let n = parse_notify_icon_data(&b.buf).unwrap();
        assert_eq!(n.tip.len(), 128);
        assert_eq!(n.state, 0x55);
    }

    #[test]
    fn non_ascii_strings() {
        let mut b = NidBuilder::new(NID32_SIZE);
        b.wstr(24, "Ses düzeyi: %50 \u{1F50A}");
        let n = parse_notify_icon_data(&b.buf).unwrap();
        assert_eq!(n.tip, "Ses düzeyi: %50 \u{1F50A}");
    }

    #[test]
    fn icon_identifier() {
        let mut buf = vec![0u8; 40];
        buf[0..4].copy_from_slice(&0x3475_3423u32.to_le_bytes());
        buf[4..8].copy_from_slice(&2u32.to_le_bytes());
        buf[8..12].copy_from_slice(&40u32.to_le_bytes());
        buf[16..20].copy_from_slice(&0xABCDu32.to_le_bytes());
        buf[20..24].copy_from_slice(&9u32.to_le_bytes());
        buf[24..40].copy_from_slice(&GUID_BYTES);
        let id = parse_icon_identifier(&buf).unwrap();
        assert_eq!(id.message, 2);
        assert_eq!(id.hwnd, 0xABCD);
        assert_eq!(id.uid, 9);
        assert_eq!(id.guid.as_deref(), Some(GUID_STR));
        // Without the guid bytes.
        let id = parse_icon_identifier(&buf[..24]).unwrap();
        assert_eq!(id.guid, None);
    }

    #[test]
    fn make_long_packs_signed() {
        assert_eq!(make_long(1, 2), 0x0002_0001);
        assert_eq!(make_long(-1, 5), 0x0005_FFFF);
        assert_eq!(make_long(100, -20), 0xFFEC_0064);
    }
}
