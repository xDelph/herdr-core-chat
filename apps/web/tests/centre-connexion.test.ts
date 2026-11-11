/**
 * Le centre de connexion (lot 4, maquette B) : chaque maillon dit son état et
 * sa cause ; un maillon dont on ne sait rien n'est jamais réputé sain.
 */
import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { Health } from "../src/api.ts";
import { diagnostic, maillons, pireTon, RETARD_DIRECT_MS } from "../src/centre-connexion.ts";
import { creeMagasinDonnees, type EtatDonnees, type OuvreEvenements } from "../src/donnees.ts";
import { Connexion } from "../src/feuilles/Connexion.tsx";

const MAINTENANT = 10_000_000;

const sain: Health = {
  herdr: { protocol: 1, reachable: true, socket: "/s", version: "0.1.19" },
  ok: true,
  uptimeMs: 3 * 86_400_000,
  watcher: { errors: 0, failing: false, lastSuccessAgoMs: 1_200, running: true },
};

const donnees = (morceau: Partial<EtatDonnees> = {}): EtatDonnees => ({
  chargement: false,
  conversations: [],
  debutsTravail: new Map(),
  dernierSucces: MAINTENANT - 5_000,
  direct: "live",
  erreurKinds: null,
  erreurs: { conversations: null, health: null, overview: null },
  health: sain,
  kinds: null,
  overview: null,
  ...morceau,
});

const chaine = (morceau: Partial<EtatDonnees> = {}, derniereVie: number | null = MAINTENANT) =>
  maillons({ derniereVie, donnees: donnees(morceau), enLigne: true, maintenant: MAINTENANT });

const ton = (liste: ReturnType<typeof chaine>, cle: string) =>
  liste.find((maillon) => maillon.cle === cle)?.ton;

describe("les maillons", () => {
  test("tout répond : cinq maillons verts", () => {
    const liste = chaine();
    expect(liste.map((m) => m.cle)).toEqual(["appareil", "demon", "core", "direct", "veille"]);
    expect(pireTon(liste)).toBe("ok");
    const core = liste.find((m) => m.cle === "core");
    expect(core?.texte).toBe("0.1.19 · protocol 1");
    expect(liste.find((m) => m.cle === "demon")?.texte).toContain("up 3d 0h");
  });

  test("herdr-core injoignable dit sa cause, telle que le démon l'a dite", () => {
    const liste = chaine({
      health: {
        herdr: { error: "needs_update: core 0.1.12", reachable: false, socket: "/s" },
        ok: false,
      },
    });
    const core = liste.find((m) => m.cle === "core");
    expect(core?.ton).toBe("erreur");
    expect(core?.detail).toBe("needs_update: core 0.1.12");
    expect(pireTon(liste)).toBe("erreur");
  });

  test("une veille en panne, puis rétablie, garde sa dernière panne en détail", () => {
    const enPanne = chaine({
      health: {
        ...sain,
        watcher: {
          errors: 3,
          failing: true,
          lastError: "doc herdr-app v31: unknown field",
          lastErrorAgoMs: 4_000,
          lastSuccessAgoMs: 50_000,
          running: true,
        },
      },
    });
    const veille = enPanne.find((m) => m.cle === "veille");
    expect(veille?.ton).toBe("erreur");
    expect(veille?.texte).toContain("Failing");
    expect(veille?.texte).toContain("3 errors");
    expect(veille?.detail).toBe("doc herdr-app v31: unknown field (4s ago)");
    const retablie = chaine({
      health: {
        ...sain,
        watcher: {
          errors: 3,
          failing: false,
          lastError: "doc herdr-app v31: unknown field",
          lastSuccessAgoMs: 900,
          running: true,
        },
      },
    });
    expect(ton(retablie, "veille")).toBe("ok");
    expect(retablie.find((m) => m.cle === "veille")?.detail).toContain("unknown field");
  });

  test("une veille arrêtée ou muette : jamais réputée saine", () => {
    expect(
      ton(
        chaine({ health: { ...sain, watcher: { errors: 0, failing: false, running: false } } }),
        "veille",
      ),
    ).toBe("erreur");
    expect(ton(chaine({ health: { ...sain, watcher: undefined } }), "veille")).toBe("inconnu");
    expect(ton(chaine({ health: null, dernierSucces: null }), "veille")).toBe("inconnu");
    expect(ton(chaine({ health: null, dernierSucces: null }), "demon")).toBe("inconnu");
  });

  test("le direct : reconnexion, silence prolongé, jamais ouvert", () => {
    expect(ton(chaine({ direct: "down" }), "direct")).toBe("attention");
    expect(ton(chaine({ direct: "inconnu" }, null), "direct")).toBe("inconnu");
    const silencieux = chaine({}, MAINTENANT - RETARD_DIRECT_MS - 1_000);
    expect(ton(silencieux, "direct")).toBe("attention");
    expect(silencieux.find((m) => m.cle === "direct")?.texte).toContain("31s ago");
  });

  test("le démon qui ne répond plus dit sa cause et quand il a répondu", () => {
    const liste = chaine({ erreurs: { conversations: null, health: "HTTP 502", overview: null } });
    const demon = liste.find((m) => m.cle === "demon");
    expect(demon?.ton).toBe("erreur");
    expect(demon?.detail).toBe("HTTP 502");
    expect(demon?.texte).toContain("last read 5s ago");
  });

  test("le diagnostic ne porte que des états", () => {
    const texte = diagnostic(chaine({ direct: "down" }));
    expect(texte.split("\n")).toHaveLength(5);
    expect(texte).toContain("Live updates: Reconnecting…");
  });
});

describe("la feuille", () => {
  test("un maillon cassé se voit, et Retry relit", () => {
    const html = renderToStaticMarkup(
      createElement(Connexion, {
        derniereVie: MAINTENANT,
        donnees: donnees({ health: { herdr: { reachable: false, socket: "/s" }, ok: false } }),
        enLigne: true,
        maintenant: MAINTENANT,
        onClose: () => undefined,
        onReessayer: () => undefined,
      }),
    );
    expect(html).toContain("Something in the chain needs attention.");
    expect(html).toContain('data-maillon="core" data-ton="erreur"');
    expect(html).toContain(">Retry<");
    expect(html).toContain("Diagnostics never include message content.");
  });
});

describe("le magasin garde le dernier signe de vie sans réveiller l'app", () => {
  test("onVie note l'instant, aucun abonné n'est réveillé", () => {
    let gestionnaires: Parameters<OuvreEvenements>[0] | null = null;
    let instant = 42;
    const magasin = creeMagasinDonnees({
      lectures: {
        conversations: async () => ({ conversations: [] }),
        health: async () => sain,
        kinds: async () => [],
        overview: async () => ({
          agents: [],
          herdrProtocol: 1,
          herdrVersion: "0.1.19",
          withoutSession: 0,
          workspaces: [],
        }),
      },
      maintenant: () => instant,
      ouvreEvenements: (g) => {
        gestionnaires = g;
        return () => undefined;
      },
      planifie: () => () => undefined,
      visible: () => true,
    });
    expect(magasin.derniereVie()).toBeNull();
    magasin.demarre();
    let reveils = 0;
    magasin.souscrire(() => (reveils += 1));
    const avant = reveils;
    instant = 1_234;
    gestionnaires?.onVie?.();
    expect(magasin.derniereVie()).toBe(1_234);
    expect(reveils).toBe(avant);
    magasin.arrete();
  });
});
