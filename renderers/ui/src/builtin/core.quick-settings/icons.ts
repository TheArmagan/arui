import type { NetworkState, VolumeState } from '@arui/sdk';
import Cable from '@lucide/svelte/icons/cable';
import Globe from '@lucide/svelte/icons/globe';
import Volume from '@lucide/svelte/icons/volume';
import Volume1 from '@lucide/svelte/icons/volume-1';
import Volume2 from '@lucide/svelte/icons/volume-2';
import VolumeX from '@lucide/svelte/icons/volume-x';
import Wifi from '@lucide/svelte/icons/wifi';
import WifiOff from '@lucide/svelte/icons/wifi-off';

export function volumeIcon(v: VolumeState) {
	if (v.muted || v.level === 0) return VolumeX;
	if (v.level < 0.34) return Volume;
	if (v.level < 0.67) return Volume1;
	return Volume2;
}

export function networkIcon(n: NetworkState) {
	if (!n.connected) return WifiOff;
	if (n.kind === 'ethernet') return Cable;
	if (n.kind === 'wifi') return Wifi;
	return Globe;
}

export function networkLabel(n: NetworkState): string {
	if (!n.connected) return 'Not connected';
	const kind = { ethernet: 'Ethernet', wifi: 'Wi-Fi', cellular: 'Cellular', other: 'Network', none: 'Network' }[n.kind];
	return n.name ? `${kind}: ${n.name}` : kind;
}
