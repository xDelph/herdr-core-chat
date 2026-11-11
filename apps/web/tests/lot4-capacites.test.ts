/**
 * Lot 4 côté PWA : les épingles partagées avec Herdr.app, les sous-agents en
 * direct, les photos posées à côté du terminal, l'écran inexact et le flux
 * d'événements de la veille. Aucun réseau : `fetch` est un double.
 */
import { afterEach, describe, expect, test } from "bun:test";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { PIECE_JOINTE_MAX_OCTETS, pinAgent, uploadAttachment } from "../src/api.ts";
import { maillons } from "../src/centre-connexion.ts";
import { avecPieceJointe, Composeur } from "../src/chat/Composeur.tsx";
import { SousAgents } from "../src/chat/SousAgents.tsx";
import type { EtatDonnees } from "../src/donnees.ts";
import { LigneAgent } from "../src/inbox/LigneAgent.tsx";
import { sectionConversations } from "../src/sections.ts";

const conversation = (morceau: Partial<ConversationSummary>): ConversationSummary => ({
  agent: "claude",
  name: null,
  paneId: "t1",
  paneRank: null,
  sessionState: "available",
  status: "idle",
  tabId: "tab",
  tabPaneCount: 1,
  title: "atelier",
  workspaceId: "w1",
  ...morceau,
});

const rendLigne = (pinned: boolean | undefined) =>
  renderToStaticMarkup(
    createElement(LigneAgent, {
      conversation: conversation(pinned === undefined ? {} : { pinned }),
      courante: false,
      debutTravail: undefined,
      maintenant: 1_000,
      nonLue: false,
      projet: "atelier",
      titre: "atelier",
    }),
  );

const rendComposeur = (avecDepot: boolean) =>
  renderToStaticMarkup(
    createElement(Composeur, {
      agentNom: "Claude",
      aide: null,
      arretEnCours: false,
      onArrete: () => undefined,
      onEnvoie: () => undefined,
      paneId: "t-photo",
      travaille: false,
      ...(avecDepot ? { deposer: async () => ({ path: "/p" }) } : {}),
    }),
  );

type Demande = {
  readonly url: string;
  readonly method: string;
  readonly entetes: Headers;
  readonly corps: BodyInit | null | undefined;
};

const original = globalThis.fetch;
const demandes: Demande[] = [];

const doublure = (corps: unknown, statut = 200) => {
  demandes.length = 0;
  globalThis.fetch = Object.assign(
    async (entree: RequestInfo | URL, init?: RequestInit) => {
      demandes.push({
        corps: init?.body,
        entetes: new Headers(init?.headers),
        method: init?.method ?? "GET",
        url: typeof entree === "string" ? entree : entree instanceof URL ? entree.href : entree.url,
      });
      return new Response(JSON.stringify(corps), {
        headers: { "content-type": "application/json" },
        status: statut,
      });
    },
    { preconnect: original.preconnect },
  );
};

afterEach(() => {
  globalThis.fetch = original;
});

describe("les épingles", () => {
  test("un épinglé passe en tête de SON groupe, l'ordre choisi tient entre les autres", () => {
    const lignes = [
      conversation({ paneId: "a", timestamp: 300 }),
      conversation({ paneId: "b", pinned: true, timestamp: 100 }),
      conversation({ paneId: "c", timestamp: 200 }),
      conversation({ paneId: "d", pinned: true, timestamp: 150 }),
      // Bloqué et épinglé : il reste dans « Needs you », jamais déplacé.
      conversation({ paneId: "e", pinned: true, status: "blocked", timestamp: 50 }),
    ];
    const groupes = sectionConversations(lignes, "recent");
    expect(groupes.map((g) => [g.cle, g.conversations.map((c) => c.paneId)])).toEqual([
      ["a-traiter", ["e"]],
      ["idle", ["d", "b", "a", "c"]],
    ]);
    const parNom = sectionConversations(
      [
        conversation({ paneId: "x", title: "alpha" }),
        conversation({ paneId: "y", pinned: true, title: "zeta" }),
      ],
      "nom",
    );
    expect(parNom[0]?.conversations.map((c) => c.paneId)).toEqual(["y", "x"]);
  });

  test("la ligne d'un agent épinglé porte l'épingle, les autres non", () => {
    expect(rendLigne(true)).toContain("ligne__epingle");
    expect(rendLigne(true)).toContain("Pinned: ");
    expect(rendLigne(undefined)).not.toContain("ligne__epingle");
  });

  test("épingler est un POST JSON avec une clé d'idempotence", async () => {
    doublure({ paneId: "t7", pinned: true });
    const reponse = await pinAgent("t7", true);
    expect(reponse).toEqual({ paneId: "t7", pinned: true });
    const [demande] = demandes;
    expect(demande?.url).toBe("/api/agents/t7/pin");
    expect(demande?.method).toBe("POST");
    expect(demande?.entetes.get("content-type")).toBe("application/json");
    expect(demande?.entetes.get("idempotency-key")).not.toBeNull();
    expect(demande?.corps).toBe(JSON.stringify({ pinned: true }));
  });

  test("un refus se lit par son code stable", async () => {
    doublure({ code: "agent_gone", error: "upstream not_found: no t7" }, 404);
    await expect(pinAgent("t7", false)).rejects.toThrow(
      "The agent is gone: its terminal no longer exists",
    );
  });
});

describe("les sous-agents en direct", () => {
  test("chacun dit son état, son titre et son message, dans ses mots", () => {
    const html = renderToStaticMarkup(
      createElement(SousAgents, {
        liste: [
          { id: "agent-abc", msg: "reading files", status: "working", title: "Explore repo" },
          { id: "agent-def", status: "done" },
        ],
      }),
    );
    expect(html).toContain("2 sub-agents");
    expect(html).toContain("Explore repo");
    expect(html).toContain("reading files");
    // Sans titre : l'identifiant du record, jamais un nom inventé.
    expect(html).toContain("agent-def");
    expect(html).toContain("etat--work");
  });

  test("aucun record : rien n'est montré", () => {
    expect(renderToStaticMarkup(createElement(SousAgents, { liste: [] }))).toBe("");
  });
});

describe("les photos", () => {
  test("le chemin s'ajoute au brouillon, sur sa propre ligne", () => {
    expect(avecPieceJointe("", "/f/t1/photo.jpg")).toBe("/f/t1/photo.jpg\n");
    expect(avecPieceJointe("regarde ça  \n", "/f/t1/photo.jpg")).toBe(
      "regarde ça\n/f/t1/photo.jpg\n",
    );
  });

  test("les octets partent BRUTS, avec leur type, leur nom et une clé", async () => {
    doublure({ bytes: 4, name: "capture.png", path: "/files/t1/capture.png" });
    const fichier = new Blob([new Uint8Array([1, 2, 3, 4])], { type: "image/png" });
    const reponse = await uploadAttachment("t1", fichier, "capture.png");
    expect(reponse.path).toBe("/files/t1/capture.png");
    const [demande] = demandes;
    expect(demande?.url).toBe("/api/agents/t1/attachments?name=capture.png");
    expect(demande?.method).toBe("POST");
    expect(demande?.entetes.get("content-type")).toBe("image/png");
    expect(demande?.entetes.get("idempotency-key")).not.toBeNull();
    expect(demande?.corps).toBe(fichier);
  });

  test("un type inconnu part en octet-stream ; trop gros, rien ne part", async () => {
    doublure({ bytes: 1, name: "x", path: "/p" });
    await uploadAttachment("t1", new Blob(["x"]), "x");
    expect(demandes[0]?.entetes.get("content-type")).toBe("application/octet-stream");
    demandes.length = 0;
    const enorme = new Blob([new Uint8Array(PIECE_JOINTE_MAX_OCTETS + 1)], { type: "image/jpeg" });
    await expect(uploadAttachment("t1", enorme, "big.jpg")).rejects.toThrow("8 MB at most");
    expect(demandes).toHaveLength(0);
  });

  test("un refus du démon est dit tel quel", async () => {
    doublure({ error: "attachment too large (maximum 8388608 bytes)" }, 413);
    await expect(uploadAttachment("t1", new Blob(["x"]), "x")).rejects.toThrow(
      "attachment too large",
    );
  });

  test("le bouton n'existe qu'avec un dépôt, et rien ne s'affiche sans dépôt en cours", () => {
    expect(rendComposeur(true)).toContain('aria-label="Attach a photo"');
    expect(rendComposeur(true)).toContain('accept="image/*"');
    expect(rendComposeur(true)).not.toContain("composeur__piece");
    expect(rendComposeur(false)).not.toContain("Attach a photo");
  });
});

describe("le flux d'événements de la veille", () => {
  const MAINTENANT = 1_800_000_000_000;
  const avecVeille = (watcher: Record<string, unknown>) => {
    const donnees: EtatDonnees = {
      chargement: false,
      conversations: [],
      debutsTravail: new Map(),
      dernierSucces: MAINTENANT - 1_000,
      direct: "live",
      erreurKinds: null,
      erreurs: { conversations: null, health: null, overview: null },
      health: {
        herdr: { protocol: 1, reachable: true, socket: "/s", version: "0.1.19" },
        ok: true,
        watcher: { errors: 0, failing: false, lastSuccessAgoMs: 500, running: true, ...watcher },
      },
      kinds: null,
      overview: null,
    };
    return maillons({
      derniereVie: MAINTENANT,
      donnees,
      enLigne: true,
      maintenant: MAINTENANT,
    }).find((maillon) => maillon.cle === "veille");
  };

  test("branché : il le dit avec son dernier signe ; perdu : la veille relit toutes les 2 s", () => {
    expect(avecVeille({ events: true, lastEventAgoMs: 3_000 })?.texte).toContain(
      "core events live (last",
    );
    const perdu = avecVeille({ events: false });
    expect(perdu?.texte).toContain("polling every 2 s");
    expect(perdu?.ton).toBe("ok");
    // Un démon qui ne le dit pas : rien n'est affirmé.
    const muet = avecVeille({});
    expect(muet?.texte).not.toContain("core events");
    expect(muet?.texte).not.toContain("polling");
  });
});
