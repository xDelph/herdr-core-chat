/** Le partage de structure : une relecture identique rend les mêmes objets. */
import { describe, expect, test } from "bun:test";
import { partageStructure } from "../src/partage.ts";

describe("partage de structure", () => {
  test("tout égal : l'ancien lui-même", () => {
    const ancien = { a: [1, { b: "x" }], c: null };
    expect(partageStructure(ancien, structuredClone(ancien))).toBe(ancien);
  });

  test("un champ change : le reste garde son identité", () => {
    const ancien = { gauche: { n: 1 }, droite: { n: 2 } };
    const suivant = partageStructure(ancien, { droite: { n: 3 }, gauche: { n: 1 } });
    expect(suivant).not.toBe(ancien);
    expect(suivant.gauche).toBe(ancien.gauche);
    expect(suivant.droite).toEqual({ n: 3 });
  });

  test("les lignes s'apparient par pane, pas par position", () => {
    const p1 = { paneId: "p1", status: "idle" };
    const p2 = { paneId: "p2", status: "idle" };
    const suivant = partageStructure([p1, p2], [{ ...p2 }, { ...p1 }]);
    expect(suivant[0]).toBe(p2);
    expect(suivant[1]).toBe(p1);
  });

  test("une clé apparue ou disparue est un changement", () => {
    const ancien: Record<string, unknown> = { a: 1 };
    expect(partageStructure(ancien, { a: 1, b: 2 })).not.toBe(ancien);
    expect(partageStructure({ a: 1, b: 2 }, { a: 1 })).toEqual({ a: 1 });
    expect(partageStructure([1, 2], [1])).toEqual([1]);
  });

  test("null et valeurs simples", () => {
    expect(partageStructure<unknown>(null, { a: 1 })).toEqual({ a: 1 });
    expect(partageStructure<unknown>({ a: 1 }, null)).toBeNull();
    expect(partageStructure("x", "x")).toBe("x");
  });
});
