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
  assert.equal(gaps.path, "M4.00,30.00 M236.00,8.00");
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

test("rounded sparklines retain every observation and cannot overshoot adjacent values", () => {
  const values = [0, 100, 0, 1, 99, 100, 100, 40, 0];
  const spark = metricSparkline(
    values.map((events, index) => ({ date: String(index), events })),
    "events",
    100,
  );
  const curves = [...spark.path.matchAll(/C([\d., ]+)/g)];
  assert.equal(curves.length, values.length - 1);
  let previous = spark.segments[0]!.start;
  curves.forEach((curve, index) => {
    const [x1, y1, x2, y2, x, y] = curve[1]!
      .trim()
      .split(/[ ,]+/)
      .map(Number) as [number, number, number, number, number, number];
    const expectedX = 4 + ((index + 1) / (values.length - 1)) * 232;
    const expectedY = 52 - (values[index + 1]! / 100) * 44;
    assert.equal(x, Number(expectedX.toFixed(2)));
    assert.equal(y, Number(expectedY.toFixed(2)));
    assert.ok(x1 > previous[0] && x2 > x1 && x > x2);
    const lower = Math.min(previous[1], y);
    const upper = Math.max(previous[1], y);
    // A cubic remains in the convex hull of its controls. All controls and
    // samples must remain in the neighbouring buckets' observed value range.
    assert.ok(y1 >= lower && y1 <= upper);
    assert.ok(y2 >= lower && y2 <= upper);
    for (let step = 0; step <= 20; step++) {
      const t = step / 20;
      const sample =
        (1 - t) ** 3 * previous[1] +
        3 * (1 - t) ** 2 * t * y1 +
        3 * (1 - t) * t ** 2 * y2 +
        t ** 3 * y;
      assert.ok(sample >= lower - 1e-10 && sample <= upper + 1e-10);
    }
    previous = [x, y];
  });
  // Local extrema have horizontal matching controls, creating rounded turns.
  assert.match(spark.path, /C13\.67,37\.33 23\.33,8\.00 33\.00,8\.00/);
  assert.match(spark.path, /C42\.67,8\.00 52\.33,52\.00 62\.00,52\.00/);
});

test("curved runs never join missing or invalid buckets and retain isolated points", () => {
  const values = [1, 4, 2, undefined, 0, NaN, 3, 7, 5];
  const spark = metricSparkline(
    values.map((events, index) => ({ date: String(index), events })),
    "events",
  );
  assert.deepEqual(
    spark.segments.map((segment) => segment.count),
    [3, 1, 3],
  );
  assert.equal((spark.path.match(/M/g) ?? []).length, 3);
  assert.equal((spark.path.match(/C/g) ?? []).length, 4);
  assert.equal(spark.point, null);
  assert.equal(spark.segments[1]!.path, "M120.00,52.00");
  assert.deepEqual(
    metricSparkline(
      [{ date: "1" }, { date: "2", events: 0 }, { date: "3" }],
      "events",
    ).point,
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
