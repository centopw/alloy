import { n as onDestroy, r as tick } from "../../chunks/index-server.js";
import { S as escape_html, a as derived, i as bind_props, l as stringify, n as attr_style, o as element, r as attributes, s as ensure_array_like, t as attr_class, x as attr } from "../../chunks/server.js";
//#region src/lib/components/Hud.svelte
function Hud($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { visible = true } = $$props;
		let fps = 60;
		let p50 = 16.6;
		let p95 = 16.6;
		let droppedFrames = 0;
		if (visible) $$renderer.push(`<!--[0--><div class="hud-panel svelte-qxof8p" data-testid="fps-hud"><div class="hud-header svelte-qxof8p"><span${attr_class("hud-dot svelte-qxof8p", void 0, {
			"healthy": true,
			"warning": false,
			"danger": false
		})}></span> <span class="hud-title svelte-qxof8p">ALLOY PERF HUD</span> <button class="hud-close svelte-qxof8p" title="Close HUD (F2 to toggle)">×</button></div> <div class="hud-metrics svelte-qxof8p"><div class="metric svelte-qxof8p"><span class="label svelte-qxof8p">FPS</span> <span${attr_class("value svelte-qxof8p", void 0, {
			"good": true,
			"warn": false
		})}>${escape_html(fps)}</span></div> <div class="metric svelte-qxof8p"><span class="label svelte-qxof8p">p50</span> <span class="value svelte-qxof8p">${escape_html(p50)} ms</span></div> <div class="metric svelte-qxof8p"><span class="label svelte-qxof8p">p95</span> <span${attr_class("value svelte-qxof8p", void 0, { "warn": false })}>${escape_html(p95)} ms</span></div> <div class="metric svelte-qxof8p"><span class="label svelte-qxof8p">Drops (>20ms)</span> <span class="value svelte-qxof8p">${escape_html(droppedFrames)}</span></div></div> <div class="hud-hint svelte-qxof8p">Press <strong>F2</strong> or <strong>Ctrl+H</strong> to toggle</div></div>`);
		else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]-->`);
		bind_props($$props, { visible });
	});
}
//#endregion
//#region node_modules/.pnpm/virtua@0.53.5_svelte@5.57.2/node_modules/virtua/lib/svelte/utils.js
var styleToString = (obj) => {
	return Object.keys(obj).reduce((acc, k) => {
		const value = obj[k];
		if (value == null) return acc;
		return acc + `${k}:${value};`;
	}, "");
}, isSameRange = (prev, next) => {
	return prev[0] === next[0] && prev[1] === next[1];
}, defaultGetKey = (_data, i) => "_" + i, t, { min: n, max: o, abs: s, floor: r } = Math, l = (t, e, s) => n(s, o(e, t)), c = (t) => t.sort((t, e) => t - e), i = setTimeout, f = clearTimeout, u = "function" == typeof queueMicrotask ? queueMicrotask : (t) => {
	Promise.resolve().then(t);
}, a = () => {
	let t;
	return [new Promise((e) => {
		t = e;
	}), t];
}, g = (t) => t.ownerDocument, d = (t) => t.defaultView, p = () => (t ??= /iP(hone|od|ad)/.test(navigator.userAgent) || "MacIntel" === navigator.platform && navigator.maxTouchPoints > 0, t), I = (t) => o(t.$getTotalSize(), t.$getViewportSize()), x = ({ $getRange: t, $findIndex: e, $getItemOffset: r, $getItemSize: l, $setItemSizes: c, $getTotalSize: i, $getLength: f, $setLength: u, $isEstimating: a }, $ = 0) => {
	let g = !!$, d = 1, m = 0, h = 0, w = 0, S = 0, b = 0, v = 0, I = 0, y = 0, x = null, z = [0, g ? o($ - 1, 0) : -1], k = !1;
	const T = /* @__PURE__ */ new Set(), M = () => w - h, L = () => M() + b + S, R = (t, e) => {
		const n = r(t), o = l(t);
		return 1 !== I && 0 === y ? n + o <= e : n < e && n + o < e + m;
	}, _ = (t) => {
		t && (p() && 0 !== I || x && 1 === y ? b += t : S += t);
	}, O = () => {
		const t = f();
		if (2 === y) return t;
		if (x && 1 === y) return n(x[0], t);
		const o = L();
		let s = e(o);
		for (; s > 0 && !R(s - 1, o);) s--;
		for (; s < t && R(s, o);) s++;
		return s;
	};
	return {
		$dispose: () => {
			T.clear();
		},
		$getStateVersion: () => d,
		$getRange: (e = 200) => {
			if (!k || g) return z;
			let s, r;
			if (v) [s, r] = z;
			else {
				let l = o(0, L()), c = l + m;
				a() || (e = o(0, e), 1 !== I && (l -= e), 2 !== I && (c += e)), [s, r] = z = t(o(0, l), o(0, c)), x && (s = n(s, x[0]), r = o(r, x[1]));
			}
			return [o(s, 0), n(r, f() - 1)];
		},
		t: O,
		$getItemOffset: (t) => r(t) - b,
		$getScrollOffset: () => w,
		$isScrolling: () => 0 !== I,
		$getViewportSize: () => m,
		$getStartSpacerSize: () => h,
		$getTotalSize: i,
		o: () => (v = S, S = 0, [v, 2 === y]),
		$subscribe: (t, e) => {
			const n = [t, e];
			return T.add(n), () => {
				T.delete(n);
			};
		},
		$update: (e, n, o) => {
			let l, a, $ = 0;
			switch (e) {
				case 1: {
					if (n === w && 0 === y) break;
					const t = v;
					v = 0;
					const e = n - w, o = s(e);
					t && o < s(t) + 1 || 0 !== y || (I = e < 0 ? 2 : 1), g && (g = !1), w = n, $ = 4;
					const r = M();
					r >= -m && r <= i() && ($ += 1, a = o > m);
					break;
				}
				case 2:
					$ = 8, 0 !== I && (l = !0, $ += 1), I = 0, y = 0, x = null;
					break;
				case 3:
					if (n.length) {
						const t = O(), e = r(t);
						c(n, m), _(r(t) - e), $ = 3, a = !0;
					}
					break;
				case 4:
					m !== n && (m || (k = a = !0), m = n, $ = 3);
					break;
				case 5:
					if (n !== f()) {
						if (o) {
							const t = i();
							u(n, !0), _(i() - t), y = 2;
						} else u(n);
						$ = 1;
					}
					break;
				case 6:
					_(n), $ = 1;
					break;
				case 7:
					h = n;
					break;
				case 8:
					y = 1;
					break;
				case 9: x = t(n, n + m), $ = 1;
			}
			$ && (d = 1 + (2147483647 & d), l && b && (S += b, b = 0), T.forEach(([t, e]) => {
				$ & t && e(a);
			}));
		}
	};
}, z = (t, e, n) => {
	const o = n ? "unshift" : "push";
	for (let n = 0; n < e; n++) t[o](-1);
	return t;
}, k = (t, e, n, o = 0, s = e - 1) => {
	let c = o;
	for (; o <= s;) {
		const e = r((o + s) / 2);
		t(e) <= n ? (c = e, o = e + 1) : s = e - 1;
	}
	return l(c, 0, e - 1);
}, T = (t, e, o) => {
	let s = o && o[1] || e || 40, r = -1, l = 0, i = !e, f = 0;
	const u = o ? o[0].slice(0, t) : [];
	z(u, t - u.length);
	const a = z([], t + 1), $ = (t) => {
		const e = u[t];
		return -1 === e ? s : e;
	}, g = (e) => {
		if (!t) return 0;
		if (r >= e) return a[e];
		r < 0 && (a[0] = 0, r = 0);
		let n = r, o = a[n];
		for (; n < e;) o += $(n), a[++n] = o;
		return r = e, o;
	};
	return {
		$getRange: (e, o) => {
			let s, r;
			return f = n(f, t - 1), g(f) <= e ? (r = k(g, t, o, f), s = k(g, t, e, f, r)) : (s = k(g, t, e, void 0, f), r = k(g, t, o, s)), f = s, [s, r];
		},
		$findIndex: (e) => k(g, t, e),
		$getItemOffset: g,
		$getItemSize: $,
		$isSizeEqual: (t, e = -1) => u[t] === e,
		$setItemSizes: (t, e) => {
			for (const [e, o] of t) l += -1 === u[e] ? o : o - $(e), u[e] = o, r = n(e, r);
			if (i && e && l > e) {
				const t = [];
				u.forEach((e) => {
					-1 !== e && e && t.push(e);
				}), r = -1, c(t);
				const e = t.length, n = e / 2 | 0;
				s = e % 2 == 0 ? (t[n - 1] + t[n]) / 2 : t[n], i = !1;
			}
		},
		$getTotalSize: () => g(t),
		$getLength: () => t,
		$setLength: (e, o) => {
			const s = e - t;
			r = o ? -1 : n(e - 1, r), t = e, s > 0 ? (z(a, s), z(u, s, o)) : (a.splice(s), o ? u.splice(0, -s) : u.splice(s));
		},
		$isEstimating: () => i,
		$snapshot: () => [u.slice(), s]
	};
}, D = (t) => {
	let e;
	return {
		l(n) {
			(e || (e = new (d(g(n))).ResizeObserver(t))).observe(n);
		},
		i(t) {
			e.unobserve(t);
		},
		u() {
			e && e.disconnect();
		}
	};
}, G = (t, e, n, o, s, r, l, c) => {
	let a, $, g = 0, d = !1, m = !1, h = !1, w = !1;
	const S = Date.now, b = s ? "scrollLeft" : "scrollTop", v = s ? "left" : "top", I = (t) => r ? -t : t, y = () => I(o[b]), x = (t, e) => {
		o.scrollTo({
			[v]: I(t),
			behavior: e ? "smooth" : "instant"
		});
	}, z = () => {
		if (d || m) return d = !1, void k();
		h = !1, t.$update(2);
	}, k = () => {
		f($), $ = i(z, 150);
	}, T = () => {
		g = S(), h && (w = !0), c && t.$update(7, c()), t.$update(1, y()), k();
	}, M = (e) => {
		if (d || !t.$isScrolling() || e.ctrlKey) return;
		const n = S() - g;
		150 > n && 50 < n && (s ? e.deltaX : e.deltaY) && (d = !0);
	}, L = () => {
		m = !0, h = w = !1;
	}, R = () => {
		m = !1, p() && (h = !0);
	};
	return n.addEventListener("scroll", T), n.addEventListener("wheel", M, { passive: !0 }), n.addEventListener("touchstart", L, { passive: !0 }), n.addEventListener("touchend", R, { passive: !0 }), n.addEventListener("touchcancel", R, { passive: !0 }), {
		$: T,
		u: () => {
			n.removeEventListener("scroll", T), n.removeEventListener("wheel", M), n.removeEventListener("touchstart", L), n.removeEventListener("touchend", R), n.removeEventListener("touchcancel", R), f($);
		},
		p: () => {
			const [e, n] = t.o();
			if (!e) return;
			w && l && l(), w = !1;
			const s = t.$getScrollOffset(), r = s + e, c = t.$getStartSpacerSize() + t.$getTotalSize() - t.$getViewportSize();
			r <= 0 || r >= c || s >= c ? x(r) : o.scrollBy({
				[v]: I(e),
				behavior: "instant"
			}), n && (a && a(), t.$getViewportSize() > t.$getTotalSize() && t.$update(1, y()));
		},
		m: (n, o) => {
			let s, r, l;
			a && a();
			const c = a = () => {
				s = !0, f(r), l && l();
			}, $ = () => {
				if (!s) {
					if (t.$getViewportSize() && (f(r), r = i(c, 150)), o) {
						for (let [n, o] = t.$getRange(0); n <= o; n++) if (e.$isSizeEqual(n)) return;
						c();
					}
					t.$update(8), x(n(), o);
				}
			};
			o && t.$update(9, n()), (() => {
				if (s) return;
				let e;
				l = t.$subscribe(2, () => {
					e || (e = !0, u(() => {
						e = !1, $();
					}));
				}), $();
			})();
		}
	};
}, Z = (t, e, n) => {
	let o, s, r = a(), l = !0;
	const c = /* @__PURE__ */ new WeakMap(), f = D((r) => {
		const i = [];
		for (const { target: f, contentRect: { width: u, height: a } } of r) if (f === o) u || a ? (l || s.$(), l = !0, t.$update(4, n ? u : a)) : l = !1;
		else if (f.offsetParent) {
			const t = c.get(f);
			if (null != t) {
				const o = n ? u : a;
				e.$isSizeEqual(t, o) || i.push([t, o]);
			}
		}
		t.$update(3, i);
	});
	return {
		$observe(l, c = l.parentElement) {
			f.l(o = c), s = G(t, e, c, c, n, n && "rtl" === getComputedStyle(c).direction, () => {
				const t = n ? "overflowX" : "overflowY", e = c.style, o = e[t];
				e[t] = "hidden", i(() => {
					e[t] = o;
				});
			}), r[1](!0);
		},
		$dispose() {
			f.u(), s && s.u(), r[1](!1), r = a();
		},
		$observeItem: (t, e) => (c.set(t, e), f.l(t), () => {
			c.delete(t), f.i(t);
		}),
		async $scroll(t, e) {
			await r[0] && s.m(t, e);
		},
		$effect() {
			s && s.p();
		},
		$getBaseOffset: t.$getStartSpacerSize
	};
}, H = (t, e) => {
	t.$scroll(() => e);
}, K = (t, e, n) => {
	H(t, n + e.$getScrollOffset());
}, N = (t, e, n, o, { align: s, smooth: r, offset: c = 0 } = {}) => {
	if (o = l(o, 0, n.$getLength() - 1), "nearest" === s) {
		const t = e.$getItemOffset(o), r = e.$getScrollOffset();
		if (t < r) s = "start";
		else {
			if (!(t + n.$getItemSize(o) > r + e.$getViewportSize())) return;
			s = "end";
		}
	}
	t.$scroll(() => c + t.$getBaseOffset() + e.$getItemOffset(o) + ("end" === s ? n.$getItemSize(o) - e.$getViewportSize() : "center" === s ? (n.$getItemSize(o) - e.$getViewportSize()) / 2 : 0), r);
};
//#endregion
//#region node_modules/.pnpm/virtua@0.53.5_svelte@5.57.2/node_modules/virtua/lib/svelte/ListItem.svelte
function ListItem($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { children, item, as = "div", index, offset, hide, horizontal, isSSR, resizer, itemProps } = $$props;
		onDestroy(() => {});
		let style = derived(() => {
			const _style = {
				contain: "layout style",
				position: hide && isSSR ? void 0 : "absolute",
				[horizontal ? "height" : "width"]: "100%",
				[horizontal ? "top" : "left"]: "0px",
				[horizontal ? "inset-inline-start" : "top"]: offset + "px",
				visibility: !hide || isSSR ? void 0 : "hidden",
				...itemProps?.style
			};
			if (horizontal) _style["display"] = "inline-flex";
			return styleToString(_style);
		});
		let itemRest = derived(() => {
			if (!itemProps) return void 0;
			const { style: _style, ...rest } = itemProps;
			return rest;
		});
		element($$renderer, as, () => {
			$$renderer.push(`${attributes({
				style: style(),
				...itemRest()
			})}`);
		}, () => {
			children($$renderer, item, index);
			$$renderer.push(`<!---->`);
		});
	});
}
//#endregion
//#region node_modules/.pnpm/virtua@0.53.5_svelte@5.57.2/node_modules/virtua/lib/svelte/Virtualizer.svelte
function Virtualizer($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { data, getKey = defaultGetKey, as = "div", item: itemAs, itemProps, scrollRef, bufferSize, itemSize, ssrCount, shift = false, horizontal = false, keepMounted, cache, startMargin = 0, children, onscroll, onscrollend, onresize } = $$props;
		const layout = T(data.length, itemSize, cache);
		const store = x(layout, ssrCount);
		const driver = Z(store, layout, horizontal);
		store.$subscribe(1, () => {
			stateVersion = store.$getStateVersion();
		});
		store.$subscribe(4, () => {
			onscroll && onscroll(store.$getScrollOffset());
		});
		store.$subscribe(8, () => {
			onscrollend && onscrollend();
		});
		store.$subscribe(2, () => {
			onresize && onresize();
		});
		let isSSR = !!ssrCount;
		let stateVersion = store.$getStateVersion();
		let prevRange;
		let range = derived(() => {
			const next = store.$getRange(bufferSize);
			if (prevRange && isSameRange(prevRange, next)) return prevRange;
			return prevRange = next;
		});
		let isScrolling = derived(() => stateVersion && store.$isScrolling());
		let totalSize = derived(() => stateVersion && store.$getTotalSize());
		let indexes = derived(() => {
			const len = stateVersion && n(data.length, layout.$getLength());
			const [start, end] = range();
			const arr = [];
			if (keepMounted) {
				const mounted = new Set(keepMounted);
				for (let i = start; i <= end; i++) mounted.add(i);
				for (const index of c([...mounted])) if (index < len) arr.push(index);
			} else for (let i = start; i <= end; i++) if (i < len) arr.push(i);
			return arr;
		});
		onDestroy(() => {
			store.$dispose();
			driver.$dispose();
		});
		const getCache = layout.$snapshot;
		const getScrollOffset = store.$getScrollOffset;
		const getScrollSize = () => I(store);
		const getViewportSize = store.$getViewportSize;
		const findItemIndex = (offset) => layout.$findIndex(offset - store.$getStartSpacerSize());
		const getItemOffset = store.$getItemOffset;
		const getItemSize = layout.$getItemSize;
		const scrollToIndex = (index, opts) => N(driver, store, layout, index, opts);
		const scrollTo = (offset) => H(driver, offset);
		const scrollBy = (offset) => K(driver, store, offset);
		let containerStyle = derived(() => styleToString({
			contain: "size style",
			"overflow-anchor": "none",
			flex: "none",
			position: "relative",
			width: horizontal ? totalSize() + "px" : "100%",
			height: horizontal ? "100%" : totalSize() + "px",
			"pointer-events": isScrolling() ? "none" : void 0
		}));
		element($$renderer, as, () => {
			$$renderer.push(`${attr_style(containerStyle())}`);
		}, () => {
			$$renderer.push(`<!--[-->`);
			const each_array = ensure_array_like(indexes());
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
				let index = each_array[$$index];
				const item = data[index];
				ListItem($$renderer, {
					children,
					item,
					index,
					as: itemAs,
					offset: stateVersion && store.$getItemOffset(index),
					hide: stateVersion && layout.$isSizeEqual(index),
					horizontal,
					isSSR,
					resizer: driver.$observeItem,
					itemProps: itemProps?.({
						item,
						index
					})
				});
			}
			$$renderer.push(`<!--]-->`);
		});
		bind_props($$props, {
			getCache,
			getScrollOffset,
			getScrollSize,
			getViewportSize,
			findItemIndex,
			getItemOffset,
			getItemSize,
			scrollToIndex,
			scrollTo,
			scrollBy
		});
	});
}
//#endregion
//#region node_modules/.pnpm/virtua@0.53.5_svelte@5.57.2/node_modules/virtua/lib/svelte/VList.svelte
function VList($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let { data, getKey, bufferSize, itemSize, ssrCount, shift, horizontal, keepMounted, cache, itemProps, children, onscroll, onscrollend, onresize, $$slots, $$events, ...rest } = $$props;
		let ref = void 0;
		const getCache = () => ref.getCache();
		const getScrollOffset = () => ref.getScrollOffset();
		const getScrollSize = () => ref.getScrollSize();
		const getViewportSize = () => ref.getViewportSize();
		const findItemIndex = (...args) => ref.findItemIndex(...args);
		const getItemOffset = (...args) => ref.getItemOffset(...args);
		const getItemSize = (...args) => ref.getItemSize(...args);
		const scrollToIndex = (...args) => ref.scrollToIndex(...args);
		const scrollTo = (...args) => ref.scrollTo(...args);
		const scrollBy = (...args) => ref.scrollBy(...args);
		const viewportStyle = styleToString({
			display: "block",
			[horizontal ? "overflow-x" : "overflow-y"]: "auto",
			contain: "strict",
			width: "100%",
			height: "100%"
		});
		$$renderer.push(`<div${attributes({
			...rest,
			style: `${stringify(viewportStyle)} ${stringify(rest.style || "")}`
		})}>`);
		Virtualizer($$renderer, {
			data,
			children,
			getKey,
			bufferSize,
			itemSize,
			ssrCount,
			shift,
			horizontal,
			keepMounted,
			cache,
			itemProps,
			onscroll,
			onscrollend,
			onresize
		});
		$$renderer.push(`<!----></div>`);
		bind_props($$props, {
			getCache,
			getScrollOffset,
			getScrollSize,
			getViewportSize,
			findItemIndex,
			getItemOffset,
			getItemSize,
			scrollToIndex,
			scrollTo,
			scrollBy
		});
	});
}
function generateMockSpaces() {
	return [{
		id: "space-alloy",
		name: "Alloy Core",
		icon: "⚡",
		rooms: [
			{
				id: "!general:alloy.chat",
				name: "#general",
				topic: "Main development channel for Alloy Matrix client",
				unreadCount: 15,
				lastMessage: "Benchmarking virtua scroll fps on macOS WKWebView...",
				lastMessageTimestamp: Date.now() - 3e4,
				isDirect: false
			},
			{
				id: "!wasm:alloy.chat",
				name: "#wasm-plugins",
				topic: "Wasmtime Component Model runtime discussions",
				unreadCount: 0,
				lastMessage: "Pulley interpreter builds on iOS!",
				lastMessageTimestamp: Date.now() - 36e5,
				isDirect: false
			},
			{
				id: "!e2ee:alloy.chat",
				name: "#e2ee-vodozemac",
				topic: "Olm/Megolm encryption implementation",
				unreadCount: 2,
				lastMessage: "Key backup session verified.",
				lastMessageTimestamp: Date.now() - 72e5,
				isDirect: false
			}
		]
	}, {
		id: "space-matrix",
		name: "Matrix Foundation",
		icon: "🌐",
		rooms: [{
			id: "!synapse:matrix.org",
			name: "#synapse-ops",
			topic: "Homeserver operations & MSCs",
			unreadCount: 0,
			lastMessage: "v1.115 released with sliding sync improvements",
			lastMessageTimestamp: Date.now() - 144e5,
			isDirect: false
		}]
	}];
}
function generateMockMembers() {
	return [
		{
			id: "@alice:matrix.org",
			displayName: "Alice (You)",
			status: "online",
			isYou: true
		},
		{
			id: "@bob:matrix.org",
			displayName: "Bob",
			status: "online"
		},
		{
			id: "@charlie:matrix.org",
			displayName: "Charlie",
			status: "idle"
		},
		{
			id: "@dave:matrix.org",
			displayName: "Dave",
			status: "offline"
		},
		{
			id: "@elena:matrix.org",
			displayName: "Elena",
			status: "online"
		}
	];
}
//#endregion
//#region src/lib/components/Timeline.svelte
function Timeline($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let events = [];
		let shift = false;
		let firstUnreadIndex = -1;
		let composerText = "";
		function handleScroll(offset) {}
		function scrollToBottom(smooth = true) {}
		function scrollToUnread() {}
		function prependHistory(batchSize = 50) {
			if (events.length === 0) return;
			shift = true;
			const firstTs = events[0].timestamp;
			const prepended = [];
			for (let i = batchSize; i > 0; i--) {
				const ts = firstTs - i * 12e4;
				prepended.push({
					id: `history-prepend-${Date.now()}-${i}`,
					sender: i % 2 === 0 ? "@alice:matrix.org" : "@bob:matrix.org",
					timestamp: ts,
					kind: "text",
					body: `[Archived message prepended] #${i} History anchor test at timestamp ${new Date(ts).toLocaleTimeString()}`
				});
			}
			events = [...prepended, ...events];
			if (firstUnreadIndex >= 0) firstUnreadIndex += batchSize;
			(/* @__PURE__ */ tick()).then(() => {
				requestAnimationFrame(() => {
					shift = false;
				});
			});
		}
		function appendMessage(customText) {
			const text = customText?.trim() || composerText.trim() || `Synthetic real-time message #${events.length + 1}`;
			const newMsg = {
				id: `live-event-${Date.now()}`,
				sender: "@you:matrix.org",
				senderAvatar: "Y",
				timestamp: Date.now(),
				kind: "text",
				body: text,
				isUnread: false
			};
			events = [...events, newMsg];
			composerText = "";
			(/* @__PURE__ */ tick()).then(() => {});
		}
		function formatTime(timestamp) {
			return new Date(timestamp).toLocaleTimeString([], {
				hour: "2-digit",
				minute: "2-digit"
			});
		}
		$$renderer.push(`<div class="timeline-container svelte-112n2zp"><div class="timeline-header svelte-112n2zp"><div class="header-info svelte-112n2zp"><span class="room-title svelte-112n2zp">#general:alloy.chat</span> <span class="room-stats svelte-112n2zp">${escape_html(events.length.toLocaleString())} events loaded</span></div> <div class="header-controls svelte-112n2zp"><button class="btn btn-secondary svelte-112n2zp" title="Test scroll anchoring by prepending 50 older messages">+50 Older (Anchor Test)</button> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> <button class="btn btn-secondary svelte-112n2zp" title="Jump to bottom">↓ Bottom</button></div></div> <div class="timeline-viewport svelte-112n2zp">`);
		if (events.length > 0) {
			$$renderer.push("<!--[0-->");
			{
				function children($$renderer, event, index) {
					if (event.kind === "day-divider") $$renderer.push(`<!--[0--><div class="day-divider svelte-112n2zp"${attr("data-event-id", event.id)}><div class="divider-line svelte-112n2zp"></div> <span class="divider-text svelte-112n2zp">${escape_html(event.body)}</span> <div class="divider-line svelte-112n2zp"></div></div>`);
					else {
						$$renderer.push(`<!--[-1--><div${attr_class("event-row svelte-112n2zp", void 0, { "unread-border": event.isUnread })}${attr("data-event-id", event.id)}${attr("data-index", index)}>`);
						if (index === firstUnreadIndex) $$renderer.push(`<!--[0--><div class="unread-banner svelte-112n2zp"><span class="svelte-112n2zp">NEW MESSAGES BELOW</span></div>`);
						else $$renderer.push("<!--[-1-->");
						$$renderer.push(`<!--]--> <div class="avatar svelte-112n2zp">${escape_html(event.senderAvatar || event.sender.slice(1, 2).toUpperCase())}</div> <div class="event-content svelte-112n2zp"><div class="event-meta svelte-112n2zp"><span class="sender-name svelte-112n2zp">${escape_html(event.sender)}</span> <span class="timestamp svelte-112n2zp">${escape_html(formatTime(event.timestamp))}</span></div> `);
						if (event.kind === "reply" && event.replyTo) $$renderer.push(`<!--[0--><div class="reply-quote svelte-112n2zp"><span class="reply-sender svelte-112n2zp">${escape_html(event.replyTo.sender)}</span> <span class="reply-body svelte-112n2zp">${escape_html(event.replyTo.snippet)}</span></div>`);
						else $$renderer.push("<!--[-1-->");
						$$renderer.push(`<!--]--> `);
						if (event.kind === "code") $$renderer.push(`<!--[0--><div class="code-wrapper svelte-112n2zp"><div class="code-header svelte-112n2zp">${escape_html(event.codeLang || "code")}</div> <pre class="code-block svelte-112n2zp"><code>${escape_html(event.body)}</code></pre></div>`);
						else if (event.kind === "image") {
							$$renderer.push(`<!--[1--><div class="image-wrapper svelte-112n2zp"><p class="image-caption svelte-112n2zp">${escape_html(event.body)}</p> `);
							if (event.imageUrl) $$renderer.push(`<!--[0--><img${attr("src", event.imageUrl)}${attr("alt", event.body)} loading="lazy" class="timeline-image svelte-112n2zp"${attr_style(`aspect-ratio: ${stringify(event.imageAspectRatio || 16 / 9)};`)}/>`);
							else $$renderer.push("<!--[-1-->");
							$$renderer.push(`<!--]--></div>`);
						} else $$renderer.push(`<!--[-1--><div class="text-body svelte-112n2zp">${escape_html(event.body)}</div>`);
						$$renderer.push(`<!--]--></div></div>`);
					}
					$$renderer.push(`<!--]-->`);
				}
				VList($$renderer, {
					data: events,
					getKey: (e) => e.id,
					shift,
					bufferSize: 300,
					onscroll: handleScroll,
					style: "height: 100%; width: 100%;",
					children,
					$$slots: { default: true }
				});
			}
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></div> <div class="composer-container svelte-112n2zp"><div class="composer-box svelte-112n2zp"><input type="text" placeholder="Send a message in #general (press Enter)..."${attr("value", composerText)} class="composer-input svelte-112n2zp"/> <button class="composer-send-btn svelte-112n2zp"${attr("disabled", !composerText.trim(), true)}>Send</button></div></div></div>`);
		bind_props($$props, {
			scrollToBottom,
			scrollToUnread,
			prependHistory,
			appendMessage
		});
	});
}
//#endregion
//#region src/lib/components/AdaptiveShell.svelte
function AdaptiveShell($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let currentTheme = "dark";
		const spaces = generateMockSpaces();
		const members = generateMockMembers();
		let activeSpaceId = spaces[0].id;
		let activeSpace = derived(() => spaces.find((s) => s.id === activeSpaceId) || spaces[0]);
		let activeRoom = spaces[0].rooms[0];
		let hudVisible = true;
		let $$settled = true;
		let $$inner_renderer;
		function $$render_inner($$renderer) {
			$$renderer.push(`<div class="alloy-shell svelte-7kc6mw"${attr("data-theme", currentTheme)}>`);
			Hud($$renderer, {
				get visible() {
					return hudVisible;
				},
				set visible($$value) {
					hudVisible = $$value;
					$$settled = false;
				}
			});
			$$renderer.push(`<!----> <aside${attr_class("pane spaces-rail svelte-7kc6mw", void 0, { "mobile-active": false })}><div class="rail-header svelte-7kc6mw"><div class="logo-badge svelte-7kc6mw" title="Alloy Matrix Shell">A</div></div> <div class="spaces-list svelte-7kc6mw"><!--[-->`);
			const each_array = ensure_array_like(spaces);
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
				let space = each_array[$$index];
				$$renderer.push(`<button${attr_class("space-item svelte-7kc6mw", void 0, { "active": space.id === activeSpaceId })}${attr("title", space.name)}><span class="space-icon">${escape_html(space.icon || space.name.slice(0, 1))}</span></button>`);
			}
			$$renderer.push(`<!--]--></div> <div class="rail-footer svelte-7kc6mw"><button class="icon-btn svelte-7kc6mw" title="Cycle theme (Dark / Light / AMOLED)">`);
			$$renderer.push(`<!--[0-->🌙`);
			$$renderer.push(`<!--]--></button> <button${attr_class("icon-btn svelte-7kc6mw", void 0, { "active": hudVisible })} title="Toggle FPS HUD (F2)">⚡</button></div></aside> <section${attr_class("pane room-list svelte-7kc6mw", void 0, { "mobile-active": false })}><div class="room-list-header svelte-7kc6mw"><div class="mobile-back svelte-7kc6mw"><button class="nav-back-btn svelte-7kc6mw">← Spaces</button></div> <div class="space-title svelte-7kc6mw"><span>${escape_html(activeSpace().name)}</span></div> <div class="search-box"><input type="text" placeholder="Filter rooms..." class="search-input svelte-7kc6mw"/></div></div> <div class="rooms-scroll svelte-7kc6mw"><div class="section-label svelte-7kc6mw">ROOMS</div> <!--[-->`);
			const each_array_1 = ensure_array_like(activeSpace().rooms);
			for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
				let room = each_array_1[$$index_1];
				$$renderer.push(`<button${attr_class("room-item svelte-7kc6mw", void 0, { "active": room.id === activeRoom.id })}><div class="room-avatar svelte-7kc6mw">#</div> <div class="room-info svelte-7kc6mw"><div class="room-row-top svelte-7kc6mw"><span class="room-name svelte-7kc6mw">${escape_html(room.name)}</span> `);
				if (room.unreadCount > 0) $$renderer.push(`<!--[0--><span class="unread-badge svelte-7kc6mw">${escape_html(room.unreadCount)}</span>`);
				else $$renderer.push("<!--[-1-->");
				$$renderer.push(`<!--]--></div> <div class="room-preview svelte-7kc6mw">${escape_html(room.lastMessage)}</div></div></button>`);
			}
			$$renderer.push(`<!--]--></div></section> <main${attr_class("pane timeline-pane svelte-7kc6mw", void 0, { "mobile-active": true })}><div class="mobile-nav-bar svelte-7kc6mw"><button class="nav-back-btn svelte-7kc6mw">← Rooms</button> <span class="mobile-room-name svelte-7kc6mw">${escape_html(activeRoom.name)}</span> <button class="nav-toggle-btn svelte-7kc6mw">Info →</button></div> <div class="timeline-wrapper svelte-7kc6mw">`);
			Timeline($$renderer, {});
			$$renderer.push(`<!----></div></main> `);
			{
				$$renderer.push(`<!--[0--><aside${attr_class("pane right-drawer svelte-7kc6mw", void 0, { "mobile-active": false })}><div class="drawer-header svelte-7kc6mw"><span class="drawer-title svelte-7kc6mw">Room Details</span> <button class="close-drawer-btn svelte-7kc6mw" title="Close details drawer">✕</button></div> <div class="drawer-content svelte-7kc6mw"><div class="drawer-section svelte-7kc6mw"><div class="detail-label svelte-7kc6mw">TOPIC</div> <p class="detail-value svelte-7kc6mw">${escape_html(activeRoom.topic || "No topic configured.")}</p></div> <div class="drawer-section svelte-7kc6mw"><div class="detail-label svelte-7kc6mw">ENCRYPTION STATUS</div> <div class="e2ee-status svelte-7kc6mw"><span class="e2ee-dot svelte-7kc6mw"></span> <span>E2EE: Vodozemac Megolm (Active)</span></div></div> <div class="drawer-section svelte-7kc6mw"><div class="detail-label svelte-7kc6mw">MEMBERS (${escape_html(members.length)})</div> <div class="member-list svelte-7kc6mw"><!--[-->`);
				const each_array_2 = ensure_array_like(members);
				for (let $$index_2 = 0, $$length = each_array_2.length; $$index_2 < $$length; $$index_2++) {
					let m = each_array_2[$$index_2];
					$$renderer.push(`<div class="member-row svelte-7kc6mw"><span${attr_class("status-indicator svelte-7kc6mw", void 0, {
						"online": m.status === "online",
						"idle": m.status === "idle"
					})}></span> <span class="member-name">${escape_html(m.displayName)}</span></div>`);
				}
				$$renderer.push(`<!--]--></div></div> <div class="drawer-section svelte-7kc6mw"><div class="detail-label svelte-7kc6mw">THEME ENGINE</div> <div class="theme-switcher-grid svelte-7kc6mw"><button${attr_class("theme-pill svelte-7kc6mw", void 0, { "selected": true })}>Dark</button> <button${attr_class("theme-pill svelte-7kc6mw", void 0, { "selected": false })}>Light</button> <button${attr_class("theme-pill svelte-7kc6mw", void 0, { "selected": false })}>AMOLED</button></div></div></div></aside>`);
			}
			$$renderer.push(`<!--]--> `);
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]--></div>`);
		}
		do {
			$$settled = true;
			$$inner_renderer = $$renderer.copy();
			$$render_inner($$inner_renderer);
		} while (!$$settled);
		$$renderer.subsume($$inner_renderer);
	});
}
//#endregion
//#region src/routes/+page.svelte
function _page($$renderer) {
	AdaptiveShell($$renderer, {});
}
//#endregion
export { _page as default };
