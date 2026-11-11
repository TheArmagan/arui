// Mock backend client: forwards commands/events to the shared MockServer and fakes the bits that
// need this window's document (DWM thumbnails are drawn as DOM overlays).

import type { CommandName, EventName, Events, Hwnd, Rect } from '@arui/sdk';
import type { ArgsOf, Backend, Unlisten } from '../types';
import { mockIconUrl } from './icons';
import { getMockServer } from './server';

function thumbLayer(): HTMLElement {
	let layer = document.getElementById('arui-mock-thumbs');
	if (!layer) {
		layer = document.createElement('div');
		layer.id = 'arui-mock-thumbs';
		layer.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2147483000;';
		document.body.appendChild(layer);
	}
	return layer;
}

function drawThumb(hwnd: Hwnd, rect: Rect, title: string, icon: string) {
	const layer = thumbLayer();
	let el = layer.querySelector<HTMLElement>(`[data-hwnd="${hwnd}"]`);
	if (!el) {
		el = document.createElement('div');
		el.dataset.hwnd = String(hwnd);
		el.innerHTML = `
			<div style="height:18px;display:flex;align-items:center;gap:6px;padding:0 6px;background:#2b2d33;font:500 10px system-ui;color:#c9cbd1">
				<img src="${icon}" style="width:11px;height:11px"/><span style="white-space:nowrap;overflow:hidden;text-overflow:ellipsis"></span>
			</div>
			<div style="flex:1;display:grid;grid-template-columns:28% 1fr;gap:6px;padding:6px;background:#1c1d21">
				<div style="border-radius:3px;background:linear-gradient(#34363d,#26282d)"></div>
				<div style="display:flex;flex-direction:column;gap:5px">
					<div style="height:22%;border-radius:3px;background:#2f3138"></div>
					<div style="flex:1;border-radius:3px;background:linear-gradient(135deg,#2c3a55,#30263f)"></div>
				</div>
			</div>`;
		el.style.cssText =
			'position:absolute;display:flex;flex-direction:column;overflow:hidden;border-radius:4px;box-shadow:0 0 0 1px #ffffff14';
		layer.appendChild(el);
	}
	el.querySelector('span')!.textContent = title;
	Object.assign(el.style, {
		left: `${rect.left}px`,
		top: `${rect.top}px`,
		width: `${rect.right - rect.left}px`,
		height: `${rect.bottom - rect.top}px`
	});
}

function clearThumbs(hwnd?: Hwnd) {
	const layer = document.getElementById('arui-mock-thumbs');
	if (!layer) return;
	if (hwnd == null) layer.replaceChildren();
	else layer.querySelector(`[data-hwnd="${hwnd}"]`)?.remove();
}

export function createMockBackend(label: string): Backend {
	const server = getMockServer();
	const conn = server.connect(label);
	window.addEventListener('pagehide', () => conn.close());

	return {
		kind: 'mock',
		label,
		async invoke<C extends CommandName>(cmd: C, ...args: ArgsOf<C>) {
			// JSON round trip strips Svelte state proxies, which structuredClone cannot copy.
			const a = args[0] === undefined ? undefined : JSON.parse(JSON.stringify(args[0]));
			if (cmd === 'preview_show') {
				const w = server.windows.find((x) => x.hwnd === a.hwnd);
				drawThumb(a.hwnd, a.rect, w?.title ?? '', mockIconUrl(w?.iconKey));
			} else if (cmd === 'preview_hide') {
				clearThumbs(a?.hwnd);
			}
			const result = await conn.invoke(cmd, a);
			// Copy into this window's realm so Svelte can proxy it (cross-realm objects are not).
			return (result === undefined ? undefined : structuredClone(result)) as any;
		},
		async listen<E extends EventName>(event: E, cb: (payload: Events[E]) => void): Promise<Unlisten> {
			return conn.on(event, (payload) => cb(structuredClone(payload) as Events[E]));
		},
		async broadcast(payload) {
			server.emit('arui://broadcast', JSON.parse(JSON.stringify(payload)));
		},
		iconUrl: mockIconUrl,
		screenRect(r) {
			// Inside the dev page every surface is an iframe and the dev desktop is the screen (scale 1).
			const frame = window.frameElement as HTMLElement | null;
			const o = frame ? frame.getBoundingClientRect() : { left: window.screenX, top: window.screenY };
			return {
				left: Math.round(o.left + r.left),
				top: Math.round(o.top + r.top),
				right: Math.round(o.left + r.right),
				bottom: Math.round(o.top + r.bottom)
			};
		}
	};
}
