/**
 * shadcn-svelte utilities. Vendored from the `utils` registry item.
 *
 * `cn` merges Tailwind class strings. The alloy design language lives in CSS custom properties
 * (`src/lib/tokens.css`), so these helpers only ever compose utility classes; no colour, radius or
 * spacing literal belongs here. See decision 0012.
 */
export { cn } from "cn";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WithoutChild<T> = T extends { child?: any } ? Omit<T, "child"> : T;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, "children"> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };
