import { describe, it, expect } from "vitest";
import { sanitizeHtml, sanitizeQuoteHtml } from "./html";

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

describe("mentions", () => {
  it("marks a matrix.to mention so it can be drawn as a pill", () => {
    const body = render('<a href="https://matrix.to/#/@alice:example.org">Alice</a>');

    expect(body.querySelector("a")?.getAttribute("class")).toBe("alloy-pill");
    expect(body.querySelector("a")?.getAttribute("href")).toBe(
      "https://matrix.to/#/@alice:example.org",
    );
  });

  it("marks an encoded room mention as well", () => {
    const body = render('<a href="https://matrix.to/#/%23general%3Aexample.org">#general</a>');

    expect(body.querySelector("a")?.getAttribute("class")).toContain("alloy-pill");
  });

  it("leaves an ordinary link a link", () => {
    const body = render('<a href="https://example.org/docs">docs</a>');

    expect(body.querySelector("a")?.hasAttribute("class")).toBe(false);
  });
});

describe("sanitizeQuoteHtml", () => {
  function renderQuote(input: string): HTMLElement {
    return new DOMParser().parseFromString(sanitizeQuoteHtml(input), "text/html").body;
  }

  it("keeps a mention looking like one, but not clickable, inside the card", () => {
    const body = renderQuote('<a href="https://matrix.to/#/@bob:example.org">Bob</a> said hi');

    expect(body.querySelector("a")).toBeNull();
    expect(body.querySelector("span.alloy-pill")?.textContent).toBe("Bob");
  });

  it("drops block markup so an excerpt stays one run of text", () => {
    const body = renderQuote("<ul><li>a</li></ul><p>b</p>");

    expect(body.querySelector("ul")).toBeNull();
    expect(body.querySelector("p")).toBeNull();
    expect(body.textContent).toBe("ab");
  });

  it("still refuses scripts and javascript: targets", () => {
    const body = renderQuote('<script>x()</script><a href="javascript:alert(1)">y</a>');

    expect(body.querySelector("script")).toBeNull();
    expect(body.textContent).not.toContain("x()");
  });
});
