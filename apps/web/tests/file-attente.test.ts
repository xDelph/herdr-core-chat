/** Les demandes en file de l'agent, reconstruites depuis la session. */
import { describe, expect, spyOn, test } from "bun:test";
import { demandesEnFile, estOperationDeFile } from "../src/file-attente.ts";

const op = (operation: string, texte: string, timestamp = "2026-10-29T17:15:08Z") => ({
  raw: JSON.stringify({
    message: {
      content: [{ text: texte, type: "text" }],
      customType: "queue-operation",
      details: { operation },
      role: "custom",
    },
    type: "message",
  }),
  timestamp,
});

describe("file d'attente de l'agent", () => {
  test("une demande mise en file attend jusqu'à son retrait", () => {
    const entrees = [op("enqueue", "le chrono a reset")];
    expect(demandesEnFile(entrees).map((d) => d.texte)).toEqual(["le chrono a reset"]);
    expect(demandesEnFile([...entrees, op("remove", "le chrono a reset")])).toEqual([]);
  });

  test("dequeue sort la tête de file", () => {
    const entrees = [op("enqueue", "a"), op("enqueue", "b"), op("dequeue", "")];
    expect(demandesEnFile(entrees).map((d) => d.texte)).toEqual(["b"]);
  });

  test("une ligne ordinaire n'est pas une opération de file", () => {
    const message = JSON.stringify({ message: { content: "x", role: "user" }, type: "message" });
    expect(estOperationDeFile(message)).toBe(false);
    expect(estOperationDeFile(op("enqueue", "a").raw)).toBe(true);
  });

  test("une ligne n'est parsée qu'une fois, et jamais sans la marque de file", () => {
    const ordinaires = Array.from({ length: 5000 }, (_, rang) => ({
      raw: JSON.stringify({ message: { content: `ligne ${rang}`, role: "user" }, type: "message" }),
    }));
    const entrees = [...ordinaires, op("enqueue", "a")];
    const parse = spyOn(JSON, "parse");
    try {
      const debut = performance.now();
      expect(demandesEnFile(entrees).map((d) => d.texte)).toEqual(["a"]);
      const premier = performance.now() - debut;
      // Seule la ligne marquée est parsée.
      expect(parse).toHaveBeenCalledTimes(1);
      parse.mockClear();
      // Un delta : les mêmes entrées + une nouvelle — rien n'est reparsé.
      const suivantes = [...entrees, op("remove", "a")];
      const debutDelta = performance.now();
      expect(demandesEnFile(suivantes)).toEqual([]);
      const delta = performance.now() - debutDelta;
      expect(parse).toHaveBeenCalledTimes(1);
      // Borne large : le chemin d'un delta reste loin des 17 ms mesurés avant.
      expect(delta).toBeLessThan(10);
      expect(premier).toBeLessThan(50);
    } finally {
      parse.mockRestore();
    }
  });
});
