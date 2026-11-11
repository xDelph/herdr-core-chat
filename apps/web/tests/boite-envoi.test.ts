/**
 * La boîte d'envoi (lot 4, maquette D) : les envois non résolus de TOUS les
 * agents, les gestes de la bulle, jamais un renvoi sans appui.
 */
import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { entreesBoite, genreDe, resumeBoite, sousTitreBoite } from "../src/boite-envoi.ts";
import { creeMagasinEnvois, type Envoi, type Lanceur } from "../src/envois.ts";
import { envois as envoisPartages } from "../src/app/services.ts";
import type { EtatDonnees } from "../src/donnees.ts";
import { BoiteEnvoi } from "../src/feuilles/BoiteEnvoi.tsx";
import { Boite } from "../src/inbox/Boite.tsx";

const envoi = (morceau: Partial<Envoi>): Envoi => ({
  apres: null,
  creeA: 1_000,
  detail: null,
  empreinte: "e",
  failureState: null,
  id: "e1",
  pane: "t1",
  state: "uncertain",
  text: "Run the full verify",
  ...morceau,
});

const lanceConfirme: Lanceur = async () => ({ confirmation: "confirmed", kind: "sent" });

const rendu = (envois: ReadonlyArray<Envoi>, paneCourant: string | null = null): string =>
  renderToStaticMarkup(
    createElement(BoiteEnvoi, {
      envois,
      magasin: { annule: () => null, oublie: () => undefined, reessaye: () => false },
      maintenant: 1_000 + 3 * 60_000,
      onClose: () => undefined,
      onModifier: () => undefined,
      onOuvrir: () => undefined,
      paneCourant,
      titreDe: (pane) => `agent ${pane}`,
    }),
  ).replaceAll("&#x27;", "'");

describe("ce qui entre dans la boîte", () => {
  test("un envoi remis en sort, tous les autres y restent", () => {
    expect(genreDe(envoi({ state: "delivered" }))).toBeNull();
    expect(genreDe(envoi({ state: "queued" }))).toBe("en-cours");
    expect(genreDe(envoi({ state: "sending" }))).toBe("en-cours");
    expect(genreDe(envoi({ state: "uncertain" }))).toBe("non-confirme");
    expect(genreDe(envoi({ state: "failed" }))).toBe("non-envoye");
    expect(genreDe(envoi({ state: "suppressed" }))).toBe("non-envoye");
  });

  test("ce qui demande un regard d'abord, puis le plus récent", () => {
    const entrees = entreesBoite([
      envoi({ creeA: 1, id: "vieux-en-cours", state: "queued" }),
      envoi({ creeA: 5, id: "echec", state: "failed" }),
      envoi({ creeA: 2, id: "incertain-ancien", state: "uncertain" }),
      envoi({ creeA: 9, id: "incertain-recent", state: "uncertain" }),
      envoi({ creeA: 8, id: "remis", state: "delivered" }),
    ]);
    expect(entrees.map((entree) => entree.envoi.id)).toEqual([
      "incertain-recent",
      "incertain-ancien",
      "echec",
      "vieux-en-cours",
    ]);
    const resume = resumeBoite(entrees);
    expect(resume).toEqual({ aRegarder: 3, total: 4 });
    expect(sousTitreBoite(resume)).toBe("3 need a look");
    expect(sousTitreBoite({ aRegarder: 0, total: 2 })).toBe("2 sending");
    expect(sousTitreBoite({ aRegarder: 1, total: 1 })).toBe("1 needs a look");
  });

  test("le magasin rend les envois de TOUS les panes", () => {
    const magasin = creeMagasinEnvois(lanceConfirme, () => () => undefined);
    magasin.ajoute({ apres: null, creeA: 1, pane: "t1", text: "a" });
    magasin.ajoute({ apres: null, creeA: 2, pane: "t2", text: "b" });
    expect(magasin.tous().map((e) => e.pane)).toEqual(["t1", "t2"]);
    expect(magasin.liste("t2").map((e) => e.text)).toEqual(["b"]);
  });
});

describe("la feuille", () => {
  test("un non confirmé dit le risque et propose d'aller voir d'abord", () => {
    const html = rendu([envoi({})]);
    expect(html).toContain("agent t1");
    expect(html).toContain("3 min");
    expect(html).toContain("Not confirmed");
    expect(html).toContain("Check the conversation before retrying");
    expect(html).toContain("Open to check");
    expect(html).toContain("Send again (may arrive twice)");
    expect(html).toContain("Dismiss");
    expect(html).not.toContain(">Edit<");
  });

  test("un refus nommé : rien n'est parti, modifier est proposé", () => {
    const html = rendu([
      envoi({
        detail: "8 operations are already queued on t2: nothing was sent",
        failureState: "erreur",
        pane: "t2",
        state: "failed",
        text: "continue",
      }),
    ]);
    expect(html).toContain("Not sent");
    expect(html).toContain("8 operations are already queued");
    expect(html).toContain(">Edit<");
    // « Edit message » n'est pas un renvoi : aucun bouton de renvoi.
    expect(html).not.toContain("Retry");
    expect(html).not.toContain("may arrive twice");
  });

  test("dans la conversation ouverte, pas de « Edit » : son composeur ne le relirait pas", () => {
    const html = rendu([envoi({ failureState: "hors-ligne", pane: "t2", state: "failed" })], "t2");
    expect(html).toContain("Offline: nothing was sent.");
    expect(html).toContain(">Retry<");
    expect(html).not.toContain(">Edit<");
  });

  test("un envoi en file s'annule, un envoi en cours ne se ferme pas", () => {
    const enFile = rendu([envoi({ state: "queued" })]);
    expect(enFile).toContain(">Cancel<");
    expect(enFile).not.toContain("Dismiss");
    const enCours = rendu([envoi({ state: "sending" })]);
    expect(enCours).not.toContain("Dismiss");
    expect(enCours).not.toContain(">Cancel<");
  });

  test("vide, elle le dit", () => {
    const html = rendu([envoi({ state: "delivered" })]);
    expect(html).toContain("Nothing waiting");
    expect(html).toContain("Every message you sent has been confirmed or closed.");
  });
});

describe("l'en-tête de la boîte de réception", () => {
  const etat: EtatDonnees = {
    chargement: false,
    conversations: [],
    debutsTravail: new Map(),
    dernierSucces: null,
    direct: "live",
    erreurKinds: null,
    erreurs: { conversations: null, health: null, overview: null },
    health: null,
    kinds: null,
    overview: null,
  };
  const entete = (avecGestes: boolean): string =>
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
        ...(avecGestes ? { onBoiteEnvoi: () => undefined, onConnexion: () => undefined } : {}),
      }),
    );

  test("l'indicateur ouvre le centre de connexion ; sans envoi, pas de pastille", () => {
    const html = entete(true);
    expect(html).toContain('aria-label="Connection details"');
    expect(html).not.toContain("pastille-envois");
  });

  test("un envoi en attente montre la pastille, et l'annuler la retire", () => {
    const id = envoisPartages.ajoute({ apres: null, creeA: 1, pane: "t9", text: "x" });
    try {
      const html = entete(true);
      expect(html).toContain("pastille-envois");
      expect(html).toContain("Outbox: 1 message(s) not settled");
    } finally {
      envoisPartages.annule(id);
    }
    expect(entete(true)).not.toContain("pastille-envois");
  });

  test("sans gestes fournis, l'en-tête est celui d'avant", () => {
    expect(entete(false)).not.toContain("Connection details");
  });
});
