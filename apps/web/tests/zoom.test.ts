/**
 * Retour du gestionnaire de fenêtres de l'iPad : la meta `viewport` est réaffectée
 * pour remettre le zoom à zéro. Sans navigateur.
 */
import { describe, expect, test } from "bun:test";
import { CONTENU_VIEWPORT, reinitialiseZoom } from "../src/zoom.ts";

describe("remise à zéro du zoom au retour (iPad)", () => {
  test("réaffecte la meta, et la laisse sur sa valeur d'origine", () => {
    const meta = { content: "width=device-width, initial-scale=1, viewport-fit=cover" };
    const ecritures: string[] = [];
    const suivie = {
      get content() {
        return meta.content;
      },
      set content(valeur: string) {
        ecritures.push(valeur);
        meta.content = valeur;
      },
    };
    expect(reinitialiseZoom(suivie)).toBe(true);
    // Une valeur neutre, puis la valeur de l'application : iOS recalcule à chaque changement.
    expect(ecritures).toEqual(["width=device-width, initial-scale=1", CONTENU_VIEWPORT]);
    expect(meta.content).toBe(CONTENU_VIEWPORT);
  });

  test("sans meta, rien ne change", () => {
    expect(reinitialiseZoom(null)).toBe(false);
  });

  test("la valeur de référence est celle de index.html", async () => {
    const html = await Bun.file(new URL("../index.html", import.meta.url)).text();
    expect(html).toContain(`content="${CONTENU_VIEWPORT}"`);
  });
});
