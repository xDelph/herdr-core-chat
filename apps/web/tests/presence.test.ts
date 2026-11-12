/**
 * H23 — la présence de cet appareil : battement visible, départ caché, présence
 * signalée même sans abonnement (retour du 2026-11-12 : PWA ouverte sur la
 * tablette, sans notifications), panne signalée. Sans navigateur ni réseau.
 */
import { describe, expect, test } from "bun:test";
import { BATTEMENT_PRESENCE_MS, creePresence, type Signalement } from "../src/presence.ts";

const APPAREIL = "tablette-1";

const banc = (endpoint: string | null = "https://push.example/tel") => {
  const envois: Signalement[] = [];
  const pannes: string[] = [];
  const battements: Array<() => void> = [];
  let visible = true;
  let arrets = 0;
  let echoue = false;
  let endpointCourant = endpoint;
  const presence = creePresence({
    appareil: () => APPAREIL,
    endpoint: async () => endpointCourant,
    envoie: async (signalement) => {
      if (echoue) throw new Error("403 unauthorized origin");
      envois.push(signalement);
    },
    repete: (action, intervalle) => {
      expect(intervalle).toBe(BATTEMENT_PRESENCE_MS);
      battements.push(action);
      return () => {
        arrets += 1;
      };
    },
    signale: (cause) => pannes.push(cause),
    visible: () => visible,
  });
  return {
    arrets: () => arrets,
    battements,
    cache: () => {
      visible = false;
    },
    desabonne: () => {
      endpointCourant = null;
    },
    echoue: () => {
      echoue = true;
    },
    envois,
    pannes,
    presence,
  };
};

describe("présence de cet appareil (H23)", () => {
  test("visible : un battement tout de suite, puis à intervalle", async () => {
    const b = banc();
    await b.presence.surVisibilite();
    expect(b.envois).toEqual([
      { appareil: APPAREIL, endpoint: "https://push.example/tel", visible: true },
    ]);
    expect(b.battements).toHaveLength(1);
    // Un second signal visible ne double pas le battement.
    await b.presence.surVisibilite();
    expect(b.battements).toHaveLength(1);
  });

  test("caché : le départ part, le battement s'arrête", async () => {
    const b = banc();
    await b.presence.surVisibilite();
    b.cache();
    await b.presence.surVisibilite();
    expect(b.envois.at(-1)).toEqual({
      appareil: APPAREIL,
      endpoint: "https://push.example/tel",
      visible: false,
    });
    expect(b.arrets()).toBe(1);
  });

  test("sans abonnement push : la présence part quand même, sans endpoint", async () => {
    // La tablette ouverte retient le téléphone, même si elle n'a pas de notifications.
    const b = banc(null);
    await b.presence.surVisibilite();
    expect(b.envois).toEqual([{ appareil: APPAREIL, endpoint: null, visible: true }]);
    b.cache();
    await b.presence.surVisibilite();
    expect(b.envois.at(-1)).toEqual({ appareil: APPAREIL, endpoint: null, visible: false });
  });

  test("un départ part même si l'appareil n'a jamais battu", async () => {
    const b = banc();
    b.cache();
    await b.presence.surVisibilite();
    expect(b.envois).toEqual([{ appareil: APPAREIL, endpoint: null, visible: false }]);
  });

  test("une panne est signalée avec sa cause, jamais avalée", async () => {
    const b = banc();
    b.echoue();
    await b.presence.surVisibilite();
    expect(b.pannes).toEqual(["403 unauthorized origin"]);
  });

  test("un appareil désabonné continue de signaler sa présence", async () => {
    const b = banc();
    await b.presence.surVisibilite();
    b.desabonne();
    await b.presence.surVisibilite();
    expect(b.envois.at(-1)).toEqual({ appareil: APPAREIL, endpoint: null, visible: true });
  });
});
