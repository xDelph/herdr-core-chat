/**
 * La fenêtre glissante du fil : seuls les derniers tours sont rendus, la
 * dernière demande ne quitte jamais sa réponse, et remonter étend la fenêtre
 * avant de charger l'historique.
 */
import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { toItems, toMessages } from "../src/blocks.ts";
import { Fil } from "../src/chat/Fil.tsx";
import {
  debutEtendu,
  debutParDefaut,
  debutPourAncre,
  doitReglisser,
  ELEMENTS_MAX,
  ELEMENTS_MIN,
  MARGE_GLISSEMENT,
  TOURS_FENETRE,
} from "../src/fenetre-fil.ts";
import type { VueSession } from "../src/flux-session.ts";

/** Un fil de `tours` tours, chacun : une demande puis `reponses` réponses. */
const demandesTous = (tours: number, reponses: number) => {
  const longueur = tours * (reponses + 1);
  return { estDemande: (index: number) => index % (reponses + 1) === 0, longueur };
};

describe("fenêtre glissante du fil", () => {
  test("par défaut : les derniers tours, au moins ELEMENTS_MIN éléments", () => {
    // 100 tours de 2 éléments : 10 tours = 20 éléments < 60 → 60 éléments.
    const { estDemande, longueur } = demandesTous(100, 1);
    expect(longueur - debutParDefaut(longueur, estDemande)).toBe(ELEMENTS_MIN);
    // 100 tours de 10 éléments : 10 tours = 100 éléments.
    const longs = demandesTous(100, 9);
    expect(longs.longueur - debutParDefaut(longs.longueur, longs.estDemande)).toBe(
      TOURS_FENETRE * 10,
    );
  });

  test("au plus ELEMENTS_MAX éléments, sauf pour garder la dernière demande", () => {
    // 20 tours de 50 éléments : 10 tours = 500 > 200 → 200.
    const { estDemande, longueur } = demandesTous(20, 49);
    expect(longueur - debutParDefaut(longueur, estDemande)).toBe(ELEMENTS_MAX);
    // Un seul tour géant : la demande reste rendue avec sa réponse.
    const geant = demandesTous(1, 999);
    expect(debutParDefaut(geant.longueur, geant.estDemande)).toBe(0);
  });

  test("un fil court est rendu en entier", () => {
    const { estDemande, longueur } = demandesTous(3, 4);
    expect(debutParDefaut(longueur, estDemande)).toBe(0);
    expect(debutParDefaut(0, estDemande)).toBe(0);
  });

  test("étendre remonte de TOURS_FENETRE tours au-dessus du début, jusqu'à 0", () => {
    const { estDemande, longueur } = demandesTous(100, 9);
    const debut = debutParDefaut(longueur, estDemande);
    const etendu = debutEtendu(debut, estDemande);
    expect(debut - etendu).toBe(TOURS_FENETRE * 10);
    expect(estDemande(etendu)).toBe(true);
    expect(debutEtendu(5, estDemande)).toBe(0);
    expect(debutEtendu(0, estDemande)).toBe(0);
  });

  test("re-glisser : seulement au bas, et seulement au-delà de la marge", () => {
    expect(doitReglisser(0, ELEMENTS_MAX + MARGE_GLISSEMENT + 1, true)).toBe(true);
    expect(doitReglisser(0, ELEMENTS_MAX + MARGE_GLISSEMENT + 1, false)).toBe(false);
    expect(doitReglisser(0, ELEMENTS_MAX + MARGE_GLISSEMENT, true)).toBe(false);
  });

  test("une ancre plus haute que la fenêtre l'étend jusqu'à elle", () => {
    expect(debutPourAncre(100, 40)).toBe(40);
    expect(debutPourAncre(100, 140)).toBe(100);
    expect(debutPourAncre(100, -1)).toBe(100);
  });
});

const vue: VueSession = {
  ajoutsEnHaut: 0,
  avertissement: null,
  causeRepli: null,
  chargementAncien: false,
  entries: [],
  erreur: null,
  erreurAncien: null,
  fin: null,
  phase: "pret",
  precedent: null,
  transport: "aucun",
};

describe("le fil rend une fenêtre", () => {
  test("une longue session ne rend que ses derniers tours, avec « Show earlier »", () => {
    const raws = Array.from({ length: 400 }, (_, rang) =>
      JSON.stringify({
        message:
          rang % 2 === 0
            ? { content: [{ text: `demande ${String(rang)}`, type: "text" }], role: "user" }
            : { content: [{ text: `réponse ${String(rang)}`, type: "text" }], role: "assistant" },
        type: "message",
      }),
    );
    const items = toItems(
      toMessages(raws.map((raw, index) => ({ id: `e${String(index)}`, raw, type: "message" }))),
    );
    expect(items.length).toBe(400);
    const html = renderToStaticMarkup(
      createElement(
        Fil,
        {
          aUneSession: true,
          cwd: null,
          envoisEnAttente: 0,
          items,
          onOuvrirFichier: () => undefined,
          paneId: "p1",
          status: "idle",
          suivi: null,
          vue,
        },
        null,
      ),
    );
    const rendus = html.match(/class="fil__el[ "]/g)?.length ?? 0;
    expect(rendus).toBe(ELEMENTS_MIN);
    expect(html).toContain("réponse 399");
    expect(html).not.toContain("demande 0<");
    expect(html).toContain("Show earlier messages");
    expect(html).not.toContain("Start of the conversation");
  });
});
