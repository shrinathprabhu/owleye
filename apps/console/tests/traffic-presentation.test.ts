import assert from "node:assert/strict";
import test from "node:test";
import type { AnalyticsInterval } from "../utils/analyticsRange.ts";
import {
  chartText,
  incompleteTrafficPeriod,
  spikePublicUrl,
  trafficLineSegments,
  trafficSpikes,
} from "../utils/trafficPresentation.ts";

const source = (overrides: Record<string, unknown> = {}) => ({
  label: "X",
  evidence_type: "observed_known_referrer",
  source_domain: "www.X.com",
  public_url: null,
  positive_excess: 45,
  observed_pageviews: 60,
  ...overrides,
});

const spike = (
  date = "2026-10-16",
  overrides: Record<string, unknown> = {},
) => ({
  date,
  status: "candidate_spike",
  observed_pageviews: 80,
  expected_pageviews: 20,
  identifiable_source_pageviews: 60,
  major_source_status: "ready",
  contributors: [source()],
  ...overrides,
});

test("exact post links remain owner-supplied context, never inferred from a referrer", () => {
  const post = "https://x.com/owleye/status/12345";
  const rows = trafficSpikes(
    [
      spike("2026-10-14", {
        contributors: [source({ public_url: post })],
      }),
      spike("2026-10-15", {
        contributors: [
          source({
            label: "Launch post",
            evidence_type: "owner_supplied_campaign_link",
            public_url: post,
          }),
        ],
      }),
    ],
    ["2026-10-14", "2026-10-15"],
  );
  assert.equal(rows[0]!.sources[0]!.sourceDomain, "x.com");
  assert.equal(rows[0]!.sources[0]!.publicUrl, null);
  assert.equal(rows[1]!.sources[0]!.publicUrl, post);
  assert.equal(rows[1]!.sources[0]!.evidence, "owner_supplied_campaign_link");
  assert.equal(rows.length, 2);
  assert.deepEqual(
    {
      observed: rows[1]!.observed,
      expected: rows[1]!.expected,
      excess: rows[1]!.sources[0]!.excess,
    },
    { observed: 80, expected: 20, excess: 45 },
  );
});

test("coarse channels never become major source badges even with substantial counts or supplied-looking links", () => {
  for (const label of ["Social", "Referral", "Organic search"]) {
    const coarse = source({
      label,
      evidence_type: "observed_coarse_channel",
      public_url: "https://x.com/owleye/status/12345",
    });
    assert.deepEqual(
      trafficSpikes(
        [spike("2026-10-16", { contributors: [coarse] })],
        ["2026-10-16"],
      ),
      [],
      label,
    );
    const groups = trafficSpikes(
      [
        spike("2026-10-16", {
          contributors: [coarse, source()],
        }),
      ],
      ["2026-10-16"],
    );
    assert.deepEqual(
      groups[0]!.sources.map((item) => item.label),
      ["X"],
    );
  }
});

test("owner links reject unsafe schemes, private hosts, credentials, query data and hidden controls", () => {
  assert.equal(
    spikePublicUrl("https://www.lowkey.tools/posts/launch"),
    "https://www.lowkey.tools/posts/launch",
  );
  assert.equal(
    spikePublicUrl("https://github.com/owleye/analytics/releases"),
    "https://github.com/owleye/analytics/releases",
  );
  assert.equal(
    spikePublicUrl("https://news.ycombinator.com/item?id=8863"),
    "https://news.ycombinator.com/item?id=8863",
  );
  for (const url of [
    "https://news.ycombinator.com/item?id=8863#",
    "https://news.ycombinator.com:443/item?id=8863",
    "https://news.ycombinator.com/foo/../item?id=8863",
    "https://news.ycombinator.com/item?id=0",
    "https://news.ycombinator.com/item?id=-1",
    "https://news.ycombinator.com/item?id=01",
    "https://news.ycombinator.com/item?id=18446744073709551616",
    "https://news.ycombinator.com/item?id=8863&id=42",
    "https://news.ycombinator.com/item?id=8863&secret=42",
    "https://news.ycombinator.com/item?%69d=8863",
    "https://news.ycombinator.com/item?id=%38%38%36%33",
    "https://news.ycombinator.com/item/?id=8863",
    "https://www.news.ycombinator.com/item?id=8863",
    "https://news.ycombinator.com/item?id=8863#secret",
    undefined,
    null,
    7,
    "",
    "javascript:alert(1)",
    "data:text/html,<script>",
    "http://x.com/post",
    "//x.com/post",
    "https://user:pass@x.com/post",
    "https://x.com:8443/post",
    "https://x.com/post?email=user@example.org",
    "https://x.com/post#secret",
    "https://localhost/post",
    "https://127.0.0.1/post",
    "https://2130706433/post",
    "https://[::1]/post",
    "https://dashboard.internal/post",
    "https://service.local/post",
    "https://service.home/post",
    "https://service.lan/post",
    "https://x.test/post",
    "https://foo.-invalid.com/post",
    "https://bad..com/post",
    `https://${"a".repeat(64)}.com/post`,
    "https://x.com/post\nheader",
    "https://x.com/post%0Aheader",
    "https://x.com/post%00",
    "https://x.com/post%7F",
    "https://x.com/post\u202E",
    "https://x.com/post%E2%80%AE",
    "https://x.com/post%zz",
    `https://x.com/${"a".repeat(2000)}`,
  ])
    assert.equal(spikePublicUrl(url), null, String(url));
  const [row] = trafficSpikes(
    [
      spike("2026-10-16", {
        contributors: [
          source({
            evidence_type: "owner_supplied_campaign_link",
            source_domain: "user@x.com",
            public_url: "https://x.com/post?secret=1",
          }),
        ],
      }),
    ],
    ["2026-10-16"],
  );
  assert.equal(row!.sources[0]!.publicUrl, null);
  assert.equal(row!.sources[0]!.sourceDomain, null);
});

test("unavailable, sparse and unmatched spike days never become chart annotations", () => {
  for (const rows of [undefined, null, {}, "candidate_spike"])
    assert.deepEqual(trafficSpikes(rows, ["2026-10-16"]), []);
  const rows = [
    null,
    {},
    spike("2026-10-15"),
    ...[
      "query_failed",
      "within_initial_thresholds",
      "withheld_sparse_comparison",
      "unavailable",
      "unknown",
    ].map((status) => spike("2026-10-16", { status })),
    spike("2026-02-31"),
    spike("2026-13-01"),
    spike("2026-1-01"),
    spike("2026-10-16", { contributors: [] }),
    spike("2026-10-16", { contributors: null }),
    spike("2026-10-16", {
      contributors: [source({ evidence_type: "inferred_cause" })],
    }),
  ];
  assert.deepEqual(
    trafficSpikes(rows, [
      "2026-10-16",
      "2026-02-31",
      "2026-13-01",
      "2026-1-01",
    ]),
    [],
  );
});

test("Direct, absent campaigns, private-looking and unsafe labels are suppressed", () => {
  for (const label of [
    "",
    " ",
    "Direct",
    " direct / unknown ",
    "Direct or unknown",
    "Direct/None",
    "(direct)",
    "Unknown",
    "No campaign",
    "NONE",
    "Other",
    "user@example.org",
    "<img src=x>",
    "X\u0000",
    "X\u007F",
    "X\u202Ehidden",
    "X\u2066hidden",
    "a".repeat(121),
  ])
    assert.deepEqual(
      trafficSpikes(
        [spike("2026-10-16", { contributors: [source({ label })] })],
        ["2026-10-16"],
      ),
      [],
      label,
    );
  const [row] = trafficSpikes(
    [
      spike("2026-10-16", {
        contributors: [
          source({ label: "Direct" }),
          source({ label: "  Product Hunt  " }),
        ],
      }),
    ],
    ["2026-10-16"],
  );
  assert.equal(row!.sources[0]!.label, "Product Hunt");
});

test("finite observed values and positive source excess are required without numeric coercion", () => {
  for (const value of [
    NaN,
    Infinity,
    -Infinity,
    -1,
    null,
    "",
    "45",
    false,
    {},
  ]) {
    assert.deepEqual(
      trafficSpikes(
        [spike("2026-10-16", { observed_pageviews: value })],
        ["2026-10-16"],
      ),
      [],
    );
    assert.deepEqual(
      trafficSpikes(
        [spike("2026-10-16", { expected_pageviews: value })],
        ["2026-10-16"],
      ),
      [],
    );
    assert.deepEqual(
      trafficSpikes(
        [
          spike("2026-10-16", {
            contributors: [source({ positive_excess: value })],
          }),
        ],
        ["2026-10-16"],
      ),
      [],
    );
  }
  assert.deepEqual(
    trafficSpikes(
      [spike("2026-10-16", { contributors: [source({ positive_excess: 0 })] })],
      ["2026-10-16"],
    ),
    [],
  );
  const [row] = trafficSpikes(
    [
      spike("2026-10-16", {
        expected_pageviews: 0,
        contributors: [source({ positive_excess: 45.25 })],
      }),
    ],
    ["2026-10-16"],
  );
  assert.equal(row!.expected, 0);
  assert.equal(row!.sources[0]!.excess, 45.25);
});

test("at most four strongest spike days survive date and consecutive-group deduplication with all their sources", () => {
  const day = (date: string, excess: number, group = date) =>
    spike(date, {
      annotation_group: group,
      observed_pageviews: 100 + excess,
      identifiable_source_pageviews: 100 + excess,
      expected_pageviews: 20,
      contributors: [
        source({ positive_excess: excess, observed_pageviews: 100 + excess }),
      ],
    });
  const rows = [
    day("2026-10-10", 30, "consecutive"),
    day("2026-10-11", 100, "consecutive"),
    day("2026-10-12", 50, "duplicate-a"),
    day("2026-10-12", 90, "duplicate-b"),
    day("2026-10-13", 80),
    day("2026-10-14", 70),
    day("2026-10-15", 60),
  ];
  const dates = rows.map((row) => row.date);
  const selected = trafficSpikes(rows, dates);
  assert.deepEqual(
    selected.map((row) => [row.date, row.sources[0]!.excess]),
    [
      ["2026-10-11", 100],
      ["2026-10-12", 90],
      ["2026-10-13", 80],
      ["2026-10-14", 70],
    ],
  );
  assert.deepEqual(trafficSpikes(rows.toReversed(), dates), selected);
  assert.equal(new Set(selected.map((row) => row.date)).size, selected.length);
});

test("launch-day source groups match all four examples, including minor named traffic in the majority denominator", () => {
  const makeSource = (label: string, count: number) =>
    source({
      label,
      source_domain: {
        X: "x.com",
        Reddit: "reddit.com",
        HackerNews: "news.ycombinator.com",
        ProductHunt: "producthunt.com",
      }[label],
      observed_pageviews: count,
      positive_excess: count - 50,
      observed_share_percent: 100, // Recomputed from counts rather than trusted display data.
      evidence_type: "owner_supplied_campaign_link",
      public_url:
        label === "HackerNews"
          ? "https://news.ycombinator.com/item?id=8863"
          : `https://${label === "X" ? "x" : label.toLowerCase()}.com/posts/launch`,
    });
  const cases = [
    { counts: [5000, 2000, 500, 200], expected: ["X", "Reddit"] },
    {
      counts: [3000, 2000, 2000, 500],
      expected: ["X", "Reddit", "HackerNews"],
    },
    { counts: [5000, 200, 1000, 1000], expected: ["X"] },
    { counts: [1000, 500, 1500, 1800], expected: [] },
  ];
  const labels = ["X", "Reddit", "HackerNews", "ProductHunt"];
  for (const { counts, expected } of cases) {
    const selected = counts
      .map((count, index) => makeSource(labels[index]!, count))
      .filter((item) => item.observed_pageviews >= 2000)
      .sort((a, b) => b.observed_pageviews - a.observed_pageviews);
    const namedTotal = counts.reduce((a, b) => a + b, 0);
    const groups = trafficSpikes(
      [
        spike("2026-10-16", {
          observed_pageviews: 10000,
          expected_pageviews: 1000,
          identifiable_source_pageviews: namedTotal,
          major_source_status:
            namedTotal > 5000 ? "ready" : "identifiable_sources_not_majority",
          contributors: selected,
        }),
      ],
      ["2026-10-16"],
    );
    assert.deepEqual(
      groups.flatMap((group) => group.sources.map((item) => item.label)).sort(),
      [...expected].sort(),
    );
    assert.ok(
      groups
        .flatMap((group) => group.sources)
        .every((item) => item.publicUrl && item.share === item.observed / 100),
    );
  }
});

test("strict 2.5x, named majority and inclusive 20% source thresholds are checked independently", () => {
  const row = spike("2026-10-16", {
    observed_pageviews: 10000,
    expected_pageviews: 1000,
    identifiable_source_pageviews: 6000,
    contributors: [source({ observed_pageviews: 2000 })],
  });
  assert.equal(trafficSpikes([row], [row.date])[0]!.sources[0]!.share, 20);
  for (const change of [
    { expected_pageviews: 4000 },
    { expected_pageviews: 4001 },
    { identifiable_source_pageviews: 5000 },
    { identifiable_source_pageviews: 10001 },
    { major_source_status: "identifiable_sources_not_majority" },
    { major_source_status: undefined },
    { contributors: [source({ observed_pageviews: 1999 })] },
    { contributors: [source({ observed_pageviews: 10001 })] },
  ])
    assert.deepEqual(trafficSpikes([{ ...row, ...change }], [row.date]), []);
  assert.equal(
    trafficSpikes([{ ...row, expected_pageviews: 3999 }], [row.date]).length,
    1,
  );
});

test("same-day sources are deduplicated, ranked by traffic and capped without overlapping identified counts", () => {
  const rows = [
    spike("2026-10-16", {
      observed_pageviews: 10000,
      expected_pageviews: 1000,
      identifiable_source_pageviews: 10000,
      contributors: [
        source({
          label: "Reddit",
          source_domain: "reddit.com",
          observed_pageviews: 2000,
          positive_excess: 1900,
        }),
        source({ observed_pageviews: 5000, positive_excess: 1000 }),
        source({
          label: "X duplicate",
          observed_pageviews: 5000,
          positive_excess: 1000,
        }),
      ],
    }),
  ];
  const groups = trafficSpikes(rows, ["2026-10-16"]);
  assert.deepEqual(
    groups[0]!.sources.map((item) => item.label),
    ["X", "Reddit"],
  );
  assert.deepEqual(
    trafficSpikes(
      [
        {
          ...rows[0],
          identifiable_source_pageviews: 6000,
        },
      ],
      ["2026-10-16"],
    ),
    [],
    "overlapping or inconsistent partitions fail closed",
  );
});

test("annotation normalization bounds report rows and contributors before inspecting them", () => {
  const rejected = spike("2026-10-16", { status: "within_initial_thresholds" });
  assert.deepEqual(
    trafficSpikes([...Array(90).fill(rejected), spike()], ["2026-10-16"]),
    [],
  );
  assert.deepEqual(
    trafficSpikes(
      [
        spike("2026-10-16", {
          contributors: [
            ...Array(3).fill(source({ label: "Direct" })),
            source(),
          ],
        }),
      ],
      ["2026-10-16"],
    ),
    [],
  );
});

test("unfinished periods use UTC report time and half-open hour, day, week and month boundaries", () => {
  const periods: [AnalyticsInterval, string, string, string][] = [
    [
      "hour",
      "2026-10-16T23:37:12Z",
      "2026-10-16T23:37:12Z",
      "2026-10-17T00:37:12Z",
    ],
    ["day", "2026-10-16", "2026-10-16T00:00:00Z", "2026-10-17T00:00:00Z"],
    ["week", "2026-10-12", "2026-10-12T00:00:00Z", "2026-10-19T00:00:00Z"],
    ["month", "2026-12-01", "2026-12-01T00:00:00Z", "2027-01-01T00:00:00Z"],
    ["month", "2028-02-01", "2028-02-01T00:00:00Z", "2028-03-01T00:00:00Z"],
  ];
  for (const [interval, date, start, end] of periods) {
    assert.equal(
      incompleteTrafficPeriod(
        date,
        interval,
        new Date(Date.parse(start) - 1).toISOString(),
      ),
      false,
      `${interval}: before start`,
    );
    assert.equal(
      incompleteTrafficPeriod(date, interval, start),
      true,
      `${interval}: at start`,
    );
    assert.equal(
      incompleteTrafficPeriod(
        date,
        interval,
        new Date(Date.parse(end) - 1).toISOString(),
      ),
      true,
      `${interval}: before end`,
    );
    assert.equal(
      incompleteTrafficPeriod(date, interval, end),
      false,
      `${interval}: completed`,
    );
  }
  assert.equal(
    incompleteTrafficPeriod("2026-10-16", "day", "2026-10-17T00:30:00+05:30"),
    true,
  );
  for (const [date, interval] of [
    ["2026-02-31", "day"],
    ["2026-02-31T12:00:00Z", "hour"],
    ["invalid", "month"],
  ] as const)
    assert.equal(
      incompleteTrafficPeriod(date, interval, "2026-10-17T12:00:00Z"),
      false,
    );
  assert.equal(incompleteTrafficPeriod("2026-10-16", "day", "unknown"), false);
});

test("historical rightmost buckets remain solid; being last does not imply incompleteness", () => {
  for (const [interval, dates] of [
    ["hour", ["2026-10-16T22:37:12Z", "2026-10-16T23:37:12Z"]],
    ["day", ["2026-10-15", "2026-10-16"]],
    ["week", ["2026-09-28", "2026-10-05"]],
    ["month", ["2026-08-01", "2026-09-01"]],
  ] as [AnalyticsInterval, string[]][]) {
    assert.deepEqual(
      trafficLineSegments(dates, [2, 7], interval, "2026-10-17T12:00:00Z"),
      {
        complete: [2, 7],
        partial: [null, null],
        hasPartial: false,
      },
      interval,
    );
  }
});

test("only the connection into an unfinished bucket is dashed without changing values or bridging gaps", () => {
  const dates = ["2026-10-14", "2026-10-15", "2026-10-16", "2026-10-17"];
  const values = [6.25, null, 0, 3.75];
  Object.freeze(dates);
  Object.freeze(values);
  assert.deepEqual(
    trafficLineSegments(dates, values, "day", "2026-10-17T12:00:00Z"),
    {
      complete: [6.25, null, 0, null],
      partial: [null, null, 0, 3.75],
      hasPartial: true,
    },
  );
  assert.deepEqual(
    trafficLineSegments(
      dates,
      [6.25, 8, null, 3.75],
      "day",
      "2026-10-17T12:00:00Z",
    ),
    {
      complete: [6.25, 8, null, null],
      partial: [null, null, null, 3.75],
      hasPartial: true,
    },
  );
  assert.deepEqual(
    trafficLineSegments(
      dates,
      [6.25, 8, 0, null],
      "day",
      "2026-10-17T12:00:00Z",
    ),
    {
      complete: [6.25, 8, 0, null],
      partial: [null, null, 0, null],
      hasPartial: true,
    },
  );
  assert.deepEqual(values, [6.25, null, 0, 3.75]);
  assert.deepEqual(trafficLineSegments([], [], "day", "2026-10-17T12:00:00Z"), {
    complete: [],
    partial: [],
    hasPartial: false,
  });
});

test("tooltip text escapes HTML delimiters instead of interpreting labels as markup", () => {
  assert.equal(
    chartText(`<img src="x" onerror='alert(1)'>&`),
    "&lt;img src=&quot;x&quot; onerror=&#39;alert(1)&#39;&gt;&amp;",
  );
  assert.equal(chartText("Product Hunt"), "Product Hunt");
  assert.equal(chartText(0), "0");
});

test("registered campaign markers need no post URL and preserve the API label bound", () => {
  const label = "a".repeat(120);
  const [marker] = trafficSpikes(
    [
      spike("2026-10-16", {
        contributors: [
          source({
            label,
            evidence_type: "owner_registered_campaign",
            source_domain: null,
            public_url: "https://x.com/post",
          }),
        ],
      }),
    ],
    ["2026-10-16"],
  );
  assert.equal(marker?.sources[0]?.label, label);
  assert.equal(marker?.sources[0]?.evidence, "owner_registered_campaign");
  assert.equal(marker?.sources[0]?.sourceDomain, null);
  assert.equal(marker?.sources[0]?.publicUrl, null);
});
