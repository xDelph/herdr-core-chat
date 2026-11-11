/**
 * Le premier message d'un agent lancé d'ici : il attend l'agent prêt
 * (`idle`), jamais avant — sinon il tomberait dans le shell du lancement.
 */
import { describe, expect, test } from "bun:test";
import { creeMagasinPremiers } from "../src/premier-message.ts";

describe("le premier message attend que l'agent soit prêt", () => {
  test("rien ne sort tant que l'agent n'est pas `idle`", () => {
    const premiers = creeMagasinPremiers();
    premiers.attend("t1", "  fix the cart  ", 5);
    expect(premiers.prets([])).toEqual([]);
    expect(premiers.prets([{ paneId: "t1", status: "unknown" }])).toEqual([]);
    expect(premiers.prets([{ paneId: "t1", status: "blocked" }])).toEqual([]);
    expect(premiers.prets([{ paneId: "t1", status: "working" }])).toEqual([]);
    expect(premiers.pour("t1")?.texte).toBe("fix the cart");
  });

  test("prêt : il sort une fois, et une seule", () => {
    const premiers = creeMagasinPremiers();
    premiers.attend("t1", "fix the cart", 5);
    expect(premiers.prets([{ paneId: "t1", status: "idle" }])).toEqual([
      { creeA: 5, pane: "t1", texte: "fix the cart" },
    ]);
    expect(premiers.prets([{ paneId: "t1", status: "idle" }])).toEqual([]);
    expect(premiers.enAttente()).toBe(false);
  });

  test("annuler rend le texte au composeur", () => {
    const premiers = creeMagasinPremiers();
    premiers.attend("t1", "fix the cart", 5);
    expect(premiers.annule("t1")).toBe("fix the cart");
    expect(premiers.annule("t1")).toBeNull();
    expect(premiers.prets([{ paneId: "t1", status: "idle" }])).toEqual([]);
  });

  test("un texte vide n'attend rien", () => {
    const premiers = creeMagasinPremiers();
    premiers.attend("t1", "   ", 5);
    expect(premiers.enAttente()).toBe(false);
  });

  test("chaque changement se signale", () => {
    const premiers = creeMagasinPremiers();
    let appels = 0;
    const fin = premiers.souscrire(() => {
      appels += 1;
    });
    premiers.attend("t1", "a", 1);
    premiers.prets([{ paneId: "t1", status: "idle" }]);
    fin();
    premiers.attend("t2", "b", 1);
    expect(appels).toBe(2);
  });
});
