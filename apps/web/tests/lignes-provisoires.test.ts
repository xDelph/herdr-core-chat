/**
 * Le démarrage à froid : la liste s'affiche depuis l'overview, sans attendre
 * la lecture des sessions — et sans rien inventer.
 */
import { describe, expect, test } from "bun:test";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { Overview } from "../src/api.ts";
import type { EtatDonnees } from "../src/donnees.ts";
import { Boite } from "../src/inbox/Boite.tsx";
import { lignesAffichees } from "../src/lignes-provisoires.ts";

type Agent = Overview["agents"][number];

const agent = (morceau: Partial<Agent>): Agent => ({
  agent: "claude",
  cwd: "/x/herdr",
  focused: false,
  name: null,
  paneId: "w1:p1",
  paneRank: null,
  parentSession: null,
  sessionCwd: null,
  sessionCwdMatches: null,
  sessionId: null,
  sessionKind: null,
  sessionPath: null,
  status: "idle",
  tabId: "w1:t1",
  tabLabel: null,
  tabNumber: 1,
  tabPaneCount: 1,
  title: null,
  workspaceId: "w1",
  workspaceLabel: "herdr",
  ...morceau,
});

const overview = (agents: ReadonlyArray<Agent>): Overview => ({
  agents: [...agents],
  herdrProtocol: 22,
  herdrVersion: "0.1.19",
  withoutSession: 0,
  workspaces: [
    {
      agentPanes: [],
      agentStatus: null,
      checkoutPath: "/x/herdr",
      focused: false,
      label: "herdr",
      paneCount: 1,
      tabCount: 1,
      workspaceId: "w1",
    },
  ],
});

describe("lignes provisoires", () => {
  test("le titre suit l'ordre du démon, l'état et l'attente viennent de l'overview", () => {
    const { lignes, provisoires } = lignesAffichees(
      null,
      overview([
        agent({ paneId: "w1:p1", title: "  refactor  " }),
        agent({ paneId: "w1:p2", tabLabel: "tests", title: " " }),
        agent({ paneId: "w1:p3", workspaceLabel: null }),
        agent({ blockedKind: "permission", paneId: "w1:p4", status: "blocked", turn: 4 }),
      ]),
    );
    expect(provisoires).toBe(true);
    expect(lignes?.map((ligne) => ligne.title)).toEqual(["refactor", "tests", "w1:p3", "herdr"]);
    const bloque = lignes?.[3];
    expect(bloque?.status).toBe("blocked");
    expect(bloque?.blockedKind).toBe("permission");
    expect(bloque?.turn).toBe(4);
    // Rien d'inventé : ni aperçu, ni horodatage, ni question.
    expect(bloque?.preview).toBeUndefined();
    expect(bloque?.timestamp).toBeUndefined();
    expect(bloque?.question).toBeUndefined();
    expect(bloque?.sessionState).toBe("available");
  });

  test("la vraie liste remplace les provisoires ; sans overview, rien", () => {
    const vraie: ConversationSummary[] = [];
    expect(lignesAffichees(vraie, overview([agent({})]))).toEqual({
      lignes: vraie,
      provisoires: false,
    });
    expect(lignesAffichees(null, null)).toEqual({ lignes: null, provisoires: false });
  });
});

const donnees = (morceau: Partial<EtatDonnees>): EtatDonnees => ({
  chargement: true,
  conversations: null,
  debutsTravail: new Map(),
  dernierSucces: null,
  direct: "inconnu",
  erreurKinds: null,
  erreurs: { conversations: null, health: null, overview: null },
  health: null,
  kinds: null,
  overview: null,
  ...morceau,
});

const boite = (etat: EtatDonnees): string =>
  renderToStaticMarkup(
    createElement(Boite, {
      donnees: etat,
      groupement: "statut",
      lectures: new Map(),
      onGroupement: () => undefined,
      onLancer: () => undefined,
      onOrdre: () => undefined,
      onReessayer: () => undefined,
      onReglages: () => undefined,
      ordre: "recent",
      paneCourant: null,
      projet: null,
    }),
  ).replaceAll("&#x27;", "'");

describe("la boîte au démarrage à froid", () => {
  test("l'overview lu, la liste en vol : les agents, pas un squelette", () => {
    const html = boite(donnees({ overview: overview([agent({ title: "refactor" })]) }));
    expect(html).toContain("refactor");
    expect(html).not.toContain("Loading agents");
    expect(html).not.toContain("Connecting to the daemon");
    expect(html).not.toContain("No readable session");
    expect(html).toContain('aria-busy="true"');
  });

  test("rien de lu : le squelette, comme avant", () => {
    expect(boite(donnees({}))).toContain("squelette");
  });

  test("la liste a échoué au démarrage : les lignes restent, sans « Offline » ni squelette", () => {
    const html = boite(
      donnees({
        chargement: false,
        erreurs: { conversations: "HTTP 503", health: null, overview: null },
        overview: overview([agent({ title: "refactor" })]),
      }),
    );
    // Dans la fenêtre de démarrage, l'échec est une connexion en cours (etats.ts) ;
    // au-delà, « Unreadable list » — jamais « Offline » : rien n'a été lu.
    expect(html).not.toContain("Offline");
    expect(html).not.toContain("squelette");
    expect(html).toContain("refactor");
  });
});
