import { describe, it, expect } from "vitest";
import { sanitizeHtml } from "./html";

/** Parses sanitized output into an inert document so nothing executes or loads. */
function render(input: string): HTMLElement {
  return new DOMParser().parseFromString(sanitizeHtml(input), "text/html").body;
}

describe("sanitizeHtml", () => {
  it("removes script elements and their contents", () => {
    const body = render("<p>hi</p><script>window.pwned = true</script>");

    expect(body.querySelector("script")).toBeNull();
    expect(body.textContent).not.toContain("pwned");
    expect(body.querySelector("p")?.textContent).toBe("hi");
  });

  it("drops javascript: and data: link targets", () => {
    const js = render('<a href="javascript:alert(1)">x</a>');
    const data = render('<a href="data:text/html,<script>x</script>">y</a>');

    expect(js.querySelector("a")?.hasAttribute("href")).toBe(false);
    expect(data.querySelector("a")?.hasAttribute("href")).toBe(false);
  });

  it("keeps http(s) and mailto links", () => {
    const body = render(
      '<a href="https://example.org/a">w</a><a href="mailto:a@example.org">m</a>',
    );
    const hrefs = Array.from(body.querySelectorAll("a")).map((a) => a.getAttribute("href"));

    expect(hrefs).toEqual(["https://example.org/a", "mailto:a@example.org"]);
  });

  it("strips inline style, event handler, data-* and aria-* attributes", () => {
    const body = render(
      '<span style="background:url(https://evil.example/)" onmouseover="x()" ' +
        'data-secret="s" aria-label="l" class="c" title="t">z</span>',
    );
    const span = body.querySelector("span");

    expect(span).not.toBeNull();
    for (const name of ["style", "onmouseover", "data-secret", "aria-label"]) {
      expect(span!.hasAttribute(name)).toBe(false);
    }
    expect(span!.getAttribute("class")).toBe("c");
    expect(span!.getAttribute("title")).toBe("t");
  });

  it("removes elements outside the allowlist such as img, iframe and svg", () => {
    const body = render(
      '<img src="https://tracker.example/p.gif"><iframe src="https://evil.example"></iframe>' +
        '<svg><image href="https://x.example/y"/></svg>text',
    );

    expect(body.querySelector("img")).toBeNull();
    expect(body.querySelector("iframe")).toBeNull();
    expect(body.querySelector("svg")).toBeNull();
    expect(body.textContent).toContain("text");
  });

  it("keeps allowed formatting tags", () => {
    const body = render("<p><strong>bold</strong> and <code>x</code></p>");

    expect(body.querySelector("strong")?.textContent).toBe("bold");
    expect(body.querySelector("code")?.textContent).toBe("x");
  });

  it("keeps the Matrix spoiler marker on spans", () => {
    const body = render('<p>a <span data-mx-spoiler="why">secret</span> b</p>');

    expect(body.querySelector("span")?.hasAttribute("data-mx-spoiler")).toBe(true);
  });

  it("still drops other data attributes", () => {
    const body = render('<span data-evil="1">x</span>');

    expect(body.querySelector("span")?.hasAttribute("data-evil")).toBe(false);
  });
});
