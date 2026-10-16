import type { TimelineEvent, Room, Space, Member } from "./types.js";

const SAMPLES_USERS = [
  { id: "@alice:matrix.org", name: "Alice", avatar: "A" },
  { id: "@bob:matrix.org", name: "Bob", avatar: "B" },
  { id: "@charlie:matrix.org", name: "Charlie", avatar: "C" },
  { id: "@dave:matrix.org", name: "Dave", avatar: "D" },
  { id: "@elena:matrix.org", name: "Elena", avatar: "E" },
  { id: "@fiona:matrix.org", name: "Fiona", avatar: "F" },
];

const CODE_SNIPPETS = [
  {
    lang: "rust",
    code: `pub async fn send_event(\n    &self,\n    room_id: &RoomId,\n    content: AnyMessageLikeEventContent,\n) -> Result<OwnedEventId, MatrixError> {\n    let response = self.client.send(room_id, content).await?;\n    Ok(response.event_id)\n}`,
  },
  {
    lang: "typescript",
    code: `interface TimelineStreamOpts {\n  limit: number;\n  direction: 'forwards' | 'backwards';\n  filter?: (event: TimelineEvent) => boolean;\n}\n\nexport const stream = async (opts: TimelineStreamOpts) => {\n  return new ReadableStream({ ... });\n};`,
  },
  {
    lang: "css",
    code: `:root[data-theme="amoled"] {\n  --alloy-bg-base: #000000;\n  --alloy-bg-surface: #0a0a0a;\n  --alloy-accent: #6366f1;\n}`,
  },
  {
    lang: "shell",
    code: `pnpm tauri build --target aarch64-apple-darwin --verbose\ncargo clippy --workspace --all-targets`,
  },
];

const TEXT_PARAGRAPHS = [
  "Hey team! Just wanted to share the latest benchmarks from the Tauri 2 + Svelte 5 spike.",
  "Looks great! What about memory consumption during 10k event virtualization?",
  "Testing scroll anchoring when prepending historical messages: virtua handle maintains scroll position smoothly.",
  "Quick ping: did anyone review the sliding sync MSC4186 PR?",
  "Yes, LGTM. We should merge and run end-to-end tests against Synapse.",
  "Here is a longer breakdown of the architecture:\n1. Rust Core owns state machine and Matrix encryption keys.\n2. Svelte 5 runes handle UI reactivity.\n3. Virtual list only mounts elements in the visible viewport + buffer window.\n4. Design tokens provide dynamic theming without style recalculation overhead.",
  "Agreed. Let's make sure the FPS HUD tracks frame times under rapid touch fling.",
  "🚢 Shipped!",
  "Can you test this on an iPhone 11 class device? Gate G1 requires 60 fps scroll.",
  "Confirmed! Zero dropped frames on desktop Chromium; testing WebKit next.",
];

// Inline SVG placeholders with varied dimensions to simulate unknown-until-load images
function generateImagePlaceholder(width: number, height: number, label: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
    <rect width="${width}" height="${height}" fill="#1e293b"/>
    <text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" fill="#94a3b8" font-family="sans-serif" font-size="16">${label} (${width}x${height})</text>
  </svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

const IMAGE_VARIATIONS = [
  { width: 400, height: 200, label: "Architecture Diagram" },
  { width: 480, height: 320, label: "Benchmark Chart" },
  { width: 300, height: 400, label: "Mobile Mockup (Portrait)" },
  { width: 500, height: 160, label: "Flamegraph Summary" },
];

/**
 * Generate N synthetic timeline events.
 * Heterogeneous distribution:
 * ~65% text of varied lengths
 * ~15% replies
 * ~10% code blocks
 * ~7% images with varied aspect ratios
 * ~3% day separators
 */
export function generateSyntheticTimeline(count = 10000, startUnreadAt = 9850): TimelineEvent[] {
  const events: TimelineEvent[] = [];
  const baseTimestamp = Date.now() - count * 120_000; // 2 minutes between events
  let lastDay = -1;

  for (let i = 0; i < count; i++) {
    const ts = baseTimestamp + i * 120_000;
    const date = new Date(ts);
    const day = date.getDate();

    // Insert day separator when day rolls over
    if (day !== lastDay) {
      lastDay = day;
      events.push({
        id: `day-divider-${i}`,
        sender: "system",
        timestamp: ts,
        kind: "day-divider",
        body: date.toLocaleDateString(undefined, { weekday: "long", month: "short", day: "numeric", year: "numeric" }),
      });
    }

    const user = SAMPLES_USERS[i % SAMPLES_USERS.length];
    const isUnread = i >= startUnreadAt;
    const roll = (i * 37) % 100;

    if (roll < 65) {
      // Text event
      const text = TEXT_PARAGRAPHS[i % TEXT_PARAGRAPHS.length];
      events.push({
        id: `event-${i}`,
        sender: user.id,
        senderAvatar: user.avatar,
        timestamp: ts,
        kind: "text",
        body: text,
        isUnread,
      });
    } else if (roll < 80) {
      // Reply event
      const replyTargetIdx = Math.max(0, i - (1 + (i % 8)));
      const replyUser = SAMPLES_USERS[replyTargetIdx % SAMPLES_USERS.length];
      events.push({
        id: `event-${i}`,
        sender: user.id,
        senderAvatar: user.avatar,
        timestamp: ts,
        kind: "reply",
        body: `Replying to your point: ${TEXT_PARAGRAPHS[(i + 2) % TEXT_PARAGRAPHS.length]}`,
        replyTo: {
          id: `event-${replyTargetIdx}`,
          sender: replyUser.name,
          snippet: TEXT_PARAGRAPHS[replyTargetIdx % TEXT_PARAGRAPHS.length].slice(0, 60) + "...",
        },
        isUnread,
      });
    } else if (roll < 90) {
      // Code snippet
      const codeItem = CODE_SNIPPETS[i % CODE_SNIPPETS.length];
      events.push({
        id: `event-${i}`,
        sender: user.id,
        senderAvatar: user.avatar,
        timestamp: ts,
        kind: "code",
        codeLang: codeItem.lang,
        body: codeItem.code,
        isUnread,
      });
    } else {
      // Image event
      const img = IMAGE_VARIATIONS[i % IMAGE_VARIATIONS.length];
      events.push({
        id: `event-${i}`,
        sender: user.id,
        senderAvatar: user.avatar,
        timestamp: ts,
        kind: "image",
        body: `Shared an image: ${img.label}`,
        imageUrl: generateImagePlaceholder(img.width, img.height, img.label),
        imageAspectRatio: img.width / img.height,
        isUnread,
      });
    }
  }

  return events;
}

export function generateMockSpaces(): Space[] {
  return [
    {
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
          lastMessageTimestamp: Date.now() - 30_000,
          isDirect: false,
        },
        {
          id: "!wasm:alloy.chat",
          name: "#wasm-plugins",
          topic: "Wasmtime Component Model runtime discussions",
          unreadCount: 0,
          lastMessage: "Pulley interpreter builds on iOS!",
          lastMessageTimestamp: Date.now() - 3600_000,
          isDirect: false,
        },
        {
          id: "!e2ee:alloy.chat",
          name: "#e2ee-vodozemac",
          topic: "Olm/Megolm encryption implementation",
          unreadCount: 2,
          lastMessage: "Key backup session verified.",
          lastMessageTimestamp: Date.now() - 7200_000,
          isDirect: false,
        },
      ],
    },
    {
      id: "space-matrix",
      name: "Matrix Foundation",
      icon: "🌐",
      rooms: [
        {
          id: "!synapse:matrix.org",
          name: "#synapse-ops",
          topic: "Homeserver operations & MSCs",
          unreadCount: 0,
          lastMessage: "v1.115 released with sliding sync improvements",
          lastMessageTimestamp: Date.now() - 14400_000,
          isDirect: false,
        },
      ],
    },
  ];
}

export function generateMockMembers(): Member[] {
  return [
    { id: "@alice:matrix.org", displayName: "Alice (You)", status: "online", isYou: true },
    { id: "@bob:matrix.org", displayName: "Bob", status: "online" },
    { id: "@charlie:matrix.org", displayName: "Charlie", status: "idle" },
    { id: "@dave:matrix.org", displayName: "Dave", status: "offline" },
    { id: "@elena:matrix.org", displayName: "Elena", status: "online" },
  ];
}
