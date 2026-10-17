/** Svelte action: move the node to <body> so it escapes clipping/stacking of virtualized rows. */
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
