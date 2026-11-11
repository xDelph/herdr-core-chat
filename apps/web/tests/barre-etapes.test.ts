/** La barre des étapes : l'allure d'un tour, sans déplier — et un échec ne se dilue jamais. */
import { describe, expect, test } from "bun:test";
import { MAX_SEGMENTS, segmentsDe } from "../src/barre-etapes.ts";

describe("la barre des étapes", () => {
  test("un segment par action, l'échec en rouge, la dernière en cours", () => {
    expect(segmentsDe([false, true, false], true)).toEqual(["ok", "echec", "cours"]);
    expect(segmentsDe([false, false], false)).toEqual(["ok", "ok"]);
    expect(segmentsDe([], true)).toEqual([]);
  });

  test("au-delà du plafond, des paquets — et le paquet d'un échec reste rouge", () => {
    const echecs = Array.from({ length: 200 }, (_, index) => index === 77);
    const segments = segmentsDe(echecs, false);
    expect(segments.length).toBe(MAX_SEGMENTS);
    expect(segments.filter((segment) => segment === "echec").length).toBe(1);
  });
});
