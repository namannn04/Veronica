import { describe, expect, test } from "bun:test";

import { stopwatch, tokens } from "../src/lib/format";

describe("tokens", () => {
  test("keeps three significant figures", () => {
    expect(tokens(999)).toBe("999");
    expect(tokens(1_000)).toBe("1.00K");
    expect(tokens(1_050)).toBe("1.05K");
    expect(tokens(15_000)).toBe("15.0K");
    expect(tokens(502_000_260)).toBe("502M");
  });

  test("decides the precision and the unit after rounding", () => {
    expect(tokens(9_999)).toBe("10.0K");
    expect(tokens(99_999)).toBe("100K");
    expect(tokens(999_999)).toBe("1.00M");
    expect(tokens(999_499)).toBe("999K");
    expect(tokens(999.6)).toBe("1.00K");
  });
});

describe("stopwatch", () => {
  test("reads like a clock, seconds always shown", () => {
    expect(stopwatch(0)).toBe("0:00");
    expect(stopwatch(7)).toBe("0:07");
    expect(stopwatch(61)).toBe("1:01");
    expect(stopwatch(754)).toBe("12:34");
    expect(stopwatch(3723)).toBe("1:02:03");
    expect(stopwatch(-4)).toBe("0:00");
  });
});
