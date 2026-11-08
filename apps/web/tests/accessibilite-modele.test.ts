/**
 * P2 — lecteur d'écran, emplacement fixe de l'historique, réglages en
 * français, feuille Modèle.
 */
import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { ModelCatalogResponse } from "@herdr-core-chat/shared";
import { toMessages } from "../src/blocks.ts";
import { annonceDeStatut } from "../src/chat/Annonces.tsx";
import { Fil } from "../src/chat/Fil.tsx";
import { LIBELLE_PERMISSION } from "../src/feuilles/Reglages.tsx";
import { modeleEnSession, niveauxProposes, reflexionEnSession } from "../src/feuilles/Modele.tsx";
import type { VueSession } from "../src/flux-session.ts";
import { creeTranscripteur } from "../src/transcript.ts";
import { Notifications } from "../src/ui/Notifications.tsx";

const vue = (changement: Partial<VueSession> = {}): VueSession => ({
  ajoutsEnHaut: 0,
  avertissement: null,
  causeRepli: null,
  chargementAncien: false,
  entries: [],
  erreur: null,
  erreurAncien: null,
  fin: null,
  phase: "pret",
  precedent: "p0",
  transport: "flux",
  ...changement,
});

const ligne = (id: string, role: string, texte: string) => ({
  id,
  raw: JSON.stringify({ message: { content: texte, role }, type: "message" }),
  type: "message",
});

const items = creeTranscripteur()([
  ligne("u1", "user", "explique"),
  ligne("a1", "assistant", "Voici la réponse complète."),
]).items;

const fil = (changement: Partial<VueSession>) =>
  renderToStaticMarkup(
    createElement(
      Fil,
      {
        aUneSession: true,
        cwd: null,
        envoisEnAttente: 0,
        items,
        onOuvrirFichier: () => undefined,
        paneId: "w1:p1",
        status: "idle",
        suivi: null,
        vue: vue(changement),
      },
      null,
    ),
  ).replaceAll("&#x27;", "'");

describe("VoiceOver : le fil se parcourt, il ne se lit pas jeton par jeton", () => {
  test("le journal est muet ; une région polie existe dès le départ", () => {
    const html = fil({});
    expect(html).toContain('role="log"');
    expect(html).toContain('aria-live="off"');
    expect(html).toMatch(/<output class="sr" aria-live="polite" aria-atomic="true"><\/output>/);
  });

  test("on annonce la réponse terminée, avec son début — pas chaque morceau", () => {
    expect(annonceDeStatut("working", "done", items)).toBe(
      "Response finished: Voici la réponse complète.",
    );
    expect(annonceDeStatut("idle", "working", items)).toBe("The agent is working.");
    expect(annonceDeStatut("working", "blocked", items)).toBe("The agent needs a decision.");
    // Pas d'annonce au montage, ni sans changement.
    expect(annonceDeStatut(null, "done", items)).toBeNull();
    expect(annonceDeStatut("done", "done", items)).toBeNull();
  });

  test("la région des notifications existe même vide (sinon la première n'est pas lue)", () => {
    expect(renderToStaticMarkup(createElement(Notifications))).toBe(
      '<div class="notifs" aria-live="polite"></div>',
    );
  });
});

describe("l'historique se charge dans un emplacement de hauteur fixe", () => {
  test("même emplacement en chargement, au début, et entre les deux", () => {
    expect(fil({ chargementAncien: true })).toContain(
      '<div class="fil__haut"><output class="fil__info">Loading history…</output></div>',
    );
    expect(fil({ precedent: null })).toContain(
      '<div class="fil__haut"><p class="fil__info">Start of the conversation</p></div>',
    );
    expect(fil({})).toContain('<div class="fil__haut"></div>');
  });
});

describe("réglages en français", () => {
  test("la permission des notifications n'est jamais un mot anglais brut", () => {
    expect(LIBELLE_PERMISSION.granted).toBe("granted");
    expect(LIBELLE_PERMISSION.denied).toContain("denied");
    expect(LIBELLE_PERMISSION.default).toBe("not requested yet");
  });
});

const catalogue = (changement: Partial<ModelCatalogResponse> = {}): ModelCatalogResponse => ({
  error: null,
  kind: "pi",
  models: [{ id: "openai/gpt-5", thinkingLevels: ["low", "high"] }],
  source: "test",
  thinkingLevels: ["off", "low", "medium", "high"],
  ...changement,
});

describe("feuille Modèle : la réflexion seule, et seulement là où elle est vérifiée", () => {
  test("modèle courant hors catalogue : les niveaux du type restent proposés", () => {
    expect(niveauxProposes(catalogue(), null)).toEqual(["off", "low", "medium", "high"]);
  });

  test("sans niveaux de type, l'union des niveaux des modèles — rien d'inventé", () => {
    expect(niveauxProposes(catalogue({ thinkingLevels: [] }), null)).toEqual(["low", "high"]);
    expect(niveauxProposes(null, null)).toEqual([]);
  });

  test("un modèle choisi impose ses niveaux", () => {
    expect(niveauxProposes(catalogue(), { thinkingLevels: [] })).toEqual([]);
  });

  test("la réflexion en session : Pi, Codex et Claude — pas devin ni opencode", () => {
    // Miroir de `KINDS_REFLEXION_EN_SESSION` (interactions::reglage) : devin
    // n'a qu'Alt+T, qui fait tourner les niveaux sans en viser un.
    expect(reflexionEnSession("pi")).toBe(true);
    expect(reflexionEnSession("claude")).toBe(true);
    expect(reflexionEnSession("codex")).toBe(true);
    expect(reflexionEnSession("devin")).toBe(false);
    expect(reflexionEnSession("opencode")).toBe(false);
  });

  test("le modèle en session : pas opencode (sélecteur invérifiable)", () => {
    expect(modeleEnSession("devin")).toBe(true);
    expect(modeleEnSession("claude")).toBe(true);
    expect(modeleEnSession("opencode")).toBe(false);
    expect(modeleEnSession("cursor")).toBe(false);
  });
});

// Garde : `toMessages` reste l'entrée du décodage utilisée ailleurs.
test("le décodage des lignes de test produit bien deux messages", () => {
  expect(toMessages([ligne("u1", "user", "x"), ligne("a1", "assistant", "y")])).toHaveLength(2);
});
