/**
 * Les choix qui durent au-delà de la question demandent un second appui
 * (F1.5) ; les autres partent d'un seul.
 */
import { describe, expect, test } from "bun:test";
import { estChoixARisque } from "../src/choix-risque.ts";

describe("un choix qui dure demande un second appui", () => {
  test("les permissions durables sont reconnues", () => {
    expect(estChoixARisque("Yes, and don't ask again for rm commands in /p")).toBe(true);
    expect(estChoixARisque("Yes, allow all edits during this session")).toBe(true);
    expect(estChoixARisque("Always allow")).toBe(true);
    expect(estChoixARisque("Allow for this session")).toBe(true);
    expect(estChoixARisque("Yes", "and don’t ask again for this command")).toBe(true);
  });

  test("une réponse ponctuelle part d'un seul appui", () => {
    expect(estChoixARisque("Yes")).toBe(false);
    expect(estChoixARisque("No, and tell Claude what to do differently (esc)")).toBe(false);
    expect(estChoixARisque("Allow once")).toBe(false);
    expect(estChoixARisque("Production (I'll give password)")).toBe(false);
  });
});
