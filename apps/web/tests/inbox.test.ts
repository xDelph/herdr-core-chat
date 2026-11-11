/**
 * La boîte « Inbox » (maquette du 2026-11-10) : le logo de l'agent en tête de
 * ligne et son état en badge, deux lignes au plus, le repos replié sous autre
 * chose, « Mark all read », le bouton flottant, et la branche passée dans
 * l'en-tête de la conversation.
 */
import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { EnTete } from "../src/chat/EnTete.tsx";
import type { EtatDonnees } from "../src/donnees.ts";
import { Boite } from "../src/inbox/Boite.tsx";
import { badgeDeLigne, LigneAgent } from "../src/inbox/LigneAgent.tsx";

const conversation = (morceau: Partial<ConversationSummary>): ConversationSummary => ({
  agent: "claude",
  name: null,
  paneId: "t1",
  paneRank: null,
  sessionState: "available",
  status: "idle",
  tabId: "tab",
  tabPaneCount: 1,
  title: "Cart and checkout flow",
  workspaceId: "w1",
  ...morceau,
});

const donnees = (conversations: ReadonlyArray<ConversationSummary>): EtatDonnees => ({
  chargement: false,
  conversations,
  debutsTravail: new Map(),
  dernierSucces: 1,
  direct: "live",
  erreurKinds: null,
  erreurs: { conversations: null, health: null, overview: null },
  health: null,
  kinds: null,
  overview: {
    agents: [],
    herdrProtocol: 1,
    herdrVersion: "0.1.17",
    withoutSession: 0,
    workspaces: [
      {
        agentPanes: [],
        agentStatus: null,
        checkoutPath: "/x/orchard",
        focused: false,
        label: "orchard",
        paneCount: 1,
        tabCount: 1,
        workspaceId: "w1",
      },
    ],
  },
});

const boite = (conversations: ReadonlyArray<ConversationSummary>, avecToutLire = true) =>
  renderToStaticMarkup(
    createElement(Boite, {
      donnees: donnees(conversations),
      groupement: "statut",
      lectures: new Map(),
      onLancer: () => undefined,
      onReessayer: () => undefined,
      onReglages: () => undefined,
      ...(avecToutLire ? { onToutLire: () => undefined } : {}),
      ordre: "recent",
      paneCourant: null,
      projet: null,
    }),
  );

const ligne = (morceau: Partial<ConversationSummary>, nonLue = false) =>
  renderToStaticMarkup(
    createElement(LigneAgent, {
      conversation: conversation(morceau),
      courante: false,
      debutTravail: undefined,
      maintenant: 10_000,
      nonLue,
      projet: "orchard",
      titre: morceau.title ?? "Cart and checkout flow",
    }),
  );

describe("le badge dit l'état, posé sur le logo de l'agent", () => {
  test("à toi, erreur, au travail, non lu — rien au repos", () => {
    expect(badgeDeLigne(conversation({ status: "blocked" }), false)).toBe("question");
    expect(badgeDeLigne(conversation({ status: "error" }), false)).toBe("erreur");
    expect(badgeDeLigne(conversation({ status: "working" }), false)).toBe("travail");
    expect(badgeDeLigne(conversation({ status: "done" }), true)).toBe("non-lu");
    expect(badgeDeLigne(conversation({ status: "idle" }), false)).toBeNull();
  });

  test("le logo de l'AGENT mène la ligne, quel que soit le projet", () => {
    const codex = ligne({ agent: "codex", status: "working" });
    expect(codex).toContain("marque--codex");
    expect(codex).toContain("avatar__badge--travail");
  });
});

describe("deux lignes au plus", () => {
  test("titre et temps, puis projet · ce qui change", () => {
    const html = ligne({ lastPrompt: "rework the cart", timestamp: 4_000 });
    expect(html).toContain("Cart and checkout flow");
    expect(html).toContain("orchard");
    expect(html).toContain("rework the cart");
    expect(html).not.toContain("ligne__details");
  });

  test("un titre vide : le projet devient le titre, sans se répéter", () => {
    const html = ligne({ lastPrompt: "rework the cart", title: "" });
    expect(html.match(/orchard/g)?.length).toBe(1);
  });
});

describe("le repos se replie sous ce qui demande un regard", () => {
  test("sous un agent au travail : « N idle », lignes cachées", () => {
    const html = boite([
      conversation({ paneId: "t1", status: "working", title: "busy one" }),
      conversation({ paneId: "t2", title: "sleepy one" }),
      conversation({ paneId: "t3", title: "sleepy two" }),
    ]);
    expect(html).toContain("2 idle");
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain("sleepy one");
  });

  test("seul groupe de la liste : il reste ouvert, pas d'écran vide", () => {
    const html = boite([conversation({ paneId: "t2", title: "sleepy one" })]);
    expect(html).not.toContain("1 idle");
    expect(html).toContain("sleepy one");
  });
});

describe("« Mark all read » sur les réponses finies", () => {
  const finie = conversation({ paneId: "t4", status: "done", timestamp: 5, title: "done one" });

  test("présent quand l'app sait marquer les lectures", () => {
    const html = boite([finie]);
    expect(html).toContain("Unread");
    expect(html).toContain("Mark all read");
  });

  test("absent sinon : jamais un bouton sans effet", () => {
    expect(boite([finie], false)).not.toContain("Mark all read");
  });
});

describe("une liste sans barre d'outils", () => {
  test("ni regroupement ni tri au-dessus des agents, la recherche fermée", () => {
    const html = boite([conversation({})]);
    expect(html).not.toContain("By status");
    expect(html).not.toContain("Most recent");
    expect(html).not.toContain('type="search"');
    expect(html).toContain('aria-label="Search"');
  });

  test("le mur live est à un geste de la liste, pas dans les réglages", () => {
    expect(boite([conversation({})])).toContain('href="#/mur" aria-label="Live wall"');
  });

  test("lancer est le bouton flottant", () => {
    expect(boite([conversation({})])).toContain('class="fab" aria-label="Launch an agent"');
  });
});

describe("l'en-tête de conversation porte ce que la ligne a quitté", () => {
  test("le logo de l'agent, et projet · modèle · réflexion · branche", () => {
    const html = renderToStaticMarkup(
      createElement(EnTete, {
        branche: "develop",
        compteFichiers: () => 0,
        kind: "claude",
        modele: "claude-opus-5-5",
        onFichiers: () => undefined,
        onModele: () => undefined,
        onRetour: () => undefined,
        projet: "orchard",
        reflexion: "high",
        status: "working",
        titre: "Cart and checkout flow",
        transport: "flux",
      }),
    );
    expect(html).toContain("tete__logo");
    expect(html).toContain("orchard · claude-opus-5-5 · high · develop");
  });
});

describe("le mur live (`#/mur`)", () => {
  test("une adresse à lui, aller-retour", async () => {
    const { parseRoute, routeToHash } = await import("../src/routing.ts");
    expect(parseRoute("#/mur")).toEqual({ kind: "mur" });
    expect(routeToHash({ kind: "mur" })).toBe("#/mur");
  });

  test("à toi en carte répondable, le travail en tuile, le repos au pied", async () => {
    const { Mur } = await import("../src/mur/Mur.tsx");
    const html = renderToStaticMarkup(
      createElement(Mur, {
        donnees: donnees([
          conversation({
            paneId: "t1",
            question: { choix: [{ libelle: "Yes", numero: 1 }], contexte: [], texte: "Run it?" },
            status: "blocked",
            title: "asks one",
          }),
          conversation({ activity: "Editing items.twig", paneId: "t2", status: "working" }),
          conversation({ agent: "codex", paneId: "t3", title: "sleepy one" }),
        ]),
        lectures: new Map(),
        onQuitter: () => undefined,
      }),
    );
    expect(html).toContain("1 needs you");
    expect(html).toContain("1 working");
    expect(html).toContain("Run it?");
    expect(html).toContain("Editing items.twig");
    expect(html).toContain("1 idle");
    expect(html).toContain("marque--codex");
    // Le repos n'a pas de tuile : son titre ne s'affiche pas.
    expect(html).not.toContain("sleepy one");
  });
});

describe("des commandes en fond d'un agent qui a fini : un indice, pas un chrono", () => {
  const serveur = { agentsEnCours: 0, enCours: 1, total: 1 };

  test("la ligne montre l'indice ; au travail, non", () => {
    const html = ligne({ status: "done", tachesDeFond: serveur, timestamp: 4_000 });
    expect(html).toContain("indice-fond");
    expect(html).toContain("1 background task running");
    expect(ligne({ status: "working", tachesDeFond: serveur })).not.toContain("indice-fond");
  });

  test("dans la liste, il reste au-dessus du pli « N idle »", () => {
    const html = boite([
      conversation({ paneId: "t1", status: "working", title: "busy one" }),
      conversation({ paneId: "t2", tachesDeFond: serveur, title: "dev server one" }),
      conversation({ paneId: "t3", title: "sleepy one" }),
    ]);
    expect(html).toContain("dev server one");
    expect(html).toContain("1 idle");
    expect(html).not.toContain("sleepy one");
  });

  test("au mur : une tuile « In the background », sans chrono, hors du pied", async () => {
    const { Mur } = await import("../src/mur/Mur.tsx");
    const html = renderToStaticMarkup(
      createElement(Mur, {
        donnees: donnees([
          conversation({ paneId: "t1", tachesDeFond: serveur, title: "dev server one" }),
          conversation({ agent: "codex", paneId: "t2", title: "sleepy one" }),
        ]),
        lectures: new Map(),
        onQuitter: () => undefined,
      }),
    );
    expect(html).toContain("In the background");
    expect(html).toContain("dev server one");
    expect(html).toContain("1 background task running");
    expect(html).not.toContain("tuile__chrono");
    expect(html).toContain("1 idle");
  });
});

describe("le pied du mur : le repos par famille, jamais une ligne qui déborde", () => {
  test("un logo et un compte par famille, la plus nombreuse d'abord", async () => {
    const { famillesAuRepos } = await import("../src/mur/Mur.tsx");
    const repos = [
      ...Array.from({ length: 9 }, () => ({ agent: "claude" })),
      { agent: "pi" },
      { agent: "codex" },
      { agent: "codex" },
      { agent: null },
    ];
    expect(famillesAuRepos(repos)).toEqual([
      { kind: "claude", n: 9 },
      { kind: "codex", n: 2 },
      { kind: null, n: 1 },
      { kind: "pi", n: 1 },
    ]);
  });
});

describe("le rail du bureau filtre la même liste", () => {
  const lignes = [
    conversation({ paneId: "t1", status: "working", title: "busy one" }),
    conversation({ paneId: "t2", title: "sleepy one" }),
  ];

  test("ses comptes sont ceux des sections de la liste", async () => {
    const { Rail } = await import("../src/inbox/Rail.tsx");
    const html = renderToStaticMarkup(
      createElement(Rail, {
        donnees: donnees(lignes),
        filtre: "en-cours",
        lectures: new Map(),
        onBoiteEnvoi: () => undefined,
        onConnexion: () => undefined,
        onFiltre: () => undefined,
        onLancer: () => undefined,
        onReglages: () => undefined,
        projetCourant: null,
      }),
    );
    expect(html).toContain("All agents");
    expect(html).toContain("rail__item--en-cours rail__item--on");
    expect(html).toContain("orchard");
    expect(html).toContain("New agent");
  });

  test("filtré sur un état : ses lignes seulement, le repos déplié", () => {
    const html = renderToStaticMarkup(
      createElement(Boite, {
        donnees: donnees(lignes),
        filtreSection: "idle",
        groupement: "statut",
        lectures: new Map(),
        onLancer: () => undefined,
        onReessayer: () => undefined,
        onReglages: () => undefined,
        ordre: "recent",
        paneCourant: null,
        projet: null,
      }),
    );
    expect(html).toContain("sleepy one");
    expect(html).not.toContain("busy one");
    expect(html).not.toContain("1 idle");
  });
});
