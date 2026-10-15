import assert from "node:assert/strict";
import test from "node:test";
import { metricSparkline } from "../utils/metricSparkline.ts";

test("sparklines preserve zero, missing buckets and finite geometry", () => {
  assert.equal(metricSparkline([], "pageviews").path, "");
  assert.equal(
    metricSparkline(
      [
        { date: "1", pageviews: 0 },
        { date: "2", pageviews: 0 },
      ],
      "pageviews",
    ).path,
    "M4.00,52.00 L236.00,52.00",
  );
  const gaps = metricSparkline(
    [{ date: "1", visitors: 3 }, { date: "2" }, { date: "3", visitors: 6 }],
    "visitors",
  );
  assert.equal(gaps.path, "M4.00,30.00  M236.00,8.00");
  assert.equal(
    metricSparkline(
      [
        { date: "1", events: NaN },
        { date: "2", events: Infinity },
      ],
      "events",
    ).path,
    "",
  );
  assert.deepEqual(
    metricSparkline([{ date: "1", pageviews: 0 }], "pageviews").point,
    [120, 52],
  );
});
test("sparklines order buckets without mutating source or summing visitor uniques", () => {
  const points = [
    { date: "2026-10-02", visitors: 5 },
    { date: "2026-10-01", visitors: 10 },
  ];
  assert.equal(
    metricSparkline(points, "visitors").path,
    "M4.00,8.00 L236.00,30.00",
  );
  assert.equal(points[0]?.date, "2026-10-02");
});
