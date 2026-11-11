/**
 * Les lectures du client — sans réseau : `fetch` est un double.
 *
 * - la sonde du flux de session distingue une route absente d'un pane refusé ;
 * - une lecture a un délai, et son dépassement se dit ;
 * - les erreurs connues du démon sont traduites, la cause brute reste.
 */
import { afterEach, describe, expect, test } from "bun:test";
import {
  answerQuestion,
  changeModel,
  fetchHealth,
  fetchSession,
  LancementIncertain,
  launchAgent,
  sendPrompt,
  sondeFluxSession,
} from "../src/api.ts";
import { erreurLisible } from "../src/erreurs-demon.ts";

const original = globalThis.fetch;

const repond = (corps: string, statut: number, type: string) => {
  globalThis.fetch = Object.assign(
    async () => new Response(corps, { headers: { "content-type": type }, status: statut }),
    { preconnect: original.preconnect },
  );
};

afterEach(() => {
  globalThis.fetch = original;
});

describe("sonde du flux de session : route absente ≠ pane refusé", () => {
  const url = "/api/agents/w1%3Ap1/stream";

  test("404 JSON avec une erreur de domaine : c'est CE pane", async () => {
    repond(
      JSON.stringify({ error: "aucune source", hint: "code diagnostic : no_source" }),
      404,
      "application/json",
    );
    expect(await sondeFluxSession(url)).toEqual({ detail: "aucune source", nature: "pane" });
  });

  test("404 « route inconnue » : la route manque", async () => {
    repond(JSON.stringify({ error: "route inconnue" }), 404, "application/json");
    expect((await sondeFluxSession(url)).nature).toBe("route-absente");
  });

  test("404 « unknown route » (démon en anglais) : la route manque aussi", async () => {
    repond(JSON.stringify({ error: "unknown route" }), 404, "application/json");
    expect((await sondeFluxSession(url)).nature).toBe("route-absente");
  });

  test("405 : la route ne sert pas ce flux", async () => {
    repond(JSON.stringify({ error: "method not allowed" }), 405, "application/json");
    expect((await sondeFluxSession(url)).nature).toBe("route-absente");
  });

  test("HTML 200 (repli SPA du démon TS) : la route manque", async () => {
    repond("<!doctype html><title>Herdr Chat</title>", 200, "text/html");
    expect((await sondeFluxSession(url)).nature).toBe("route-absente");
  });

  test("un flux qui répond, ou un 503 de plafond : passager", async () => {
    repond("", 200, "text/event-stream");
    expect((await sondeFluxSession(url)).nature).toBe("passager");
    repond(JSON.stringify({ error: "trop de flux" }), 503, "application/json");
    expect((await sondeFluxSession(url)).nature).toBe("passager");
  });

  test("réseau coupé : passager, jamais une route absente", async () => {
    globalThis.fetch = Object.assign(
      async () => {
        throw new TypeError("Load failed");
      },
      { preconnect: original.preconnect },
    );
    expect((await sondeFluxSession(url)).nature).toBe("passager");
  });
});

describe("une lecture a un délai (P2)", () => {
  test("un délai dépassé se dit comme tel, pas comme une erreur réseau", async () => {
    let signal: AbortSignal | null | undefined;
    globalThis.fetch = Object.assign(
      async (_entree: RequestInfo | URL, init?: RequestInit) => {
        signal = init?.signal;
        throw new DOMException("The operation timed out.", "TimeoutError");
      },
      { preconnect: original.preconnect },
    );
    const erreur = await fetchSession("w1:p1", { tail: 1 }).catch((cause: unknown) => cause);
    expect(signal).toBeInstanceOf(AbortSignal);
    expect(erreur instanceof Error ? erreur.message : "").toBe(
      "The daemon did not respond within 10s",
    );
  });
});

describe("les erreurs du démon, en français (P2)", () => {
  test("la phrase connue est traduite, et la cause brute suit sur sa ligne", () => {
    expect(erreurLisible("no usable session path for this agent")).toBe(
      "Herdr exposes no readable session for this agent\nCause: no usable session path for this agent",
    );
  });

  test("l'arrêt sans agent est expliqué tel que le démon l'écrit", () => {
    // Le démon dit « no Herdr agent in this pane » (arret.rs) : le motif ne
    // reconnaissait que « … in pane X ».
    expect(erreurLisible("no Herdr agent in this pane")).toBe(
      "Herdr knows no agent in this pane\nCause: no Herdr agent in this pane",
    );
    expect(erreurLisible("no Herdr agent in pane w1:p2")).toBe(
      "Herdr knows no agent in pane w1:p2\nCause: no Herdr agent in pane w1:p2",
    );
    expect(erreurLisible("unreadable Herdr snapshot: socket")).toBe(
      "Herdr's state is unreadable\nCause: unreadable Herdr snapshot: socket",
    );
  });

  test("une cause inconnue passe telle quelle — rien d'inventé", () => {
    expect(erreurLisible("quelque chose d'imprévu")).toBe("quelque chose d'imprévu");
  });

  test("une lecture de session refusée affiche la traduction", async () => {
    repond(
      JSON.stringify({ error: "no usable session path for this agent" }),
      404,
      "application/json",
    );
    const erreur = await fetchSession("w1:p1").catch((cause: unknown) => cause);
    expect(erreur instanceof Error ? erreur.message : "").toContain(
      "Herdr exposes no readable session for this agent",
    );
  });

  test("hors ligne, un envoi dit « rien n'est parti »", async () => {
    const enLigne = Object.getOwnPropertyDescriptor(globalThis, "navigator");
    Object.defineProperty(globalThis, "navigator", {
      configurable: true,
      value: { onLine: false },
    });
    globalThis.fetch = Object.assign(
      async () => {
        throw new TypeError("Load failed");
      },
      { preconnect: original.preconnect },
    );
    try {
      expect(await sendPrompt("w1:p1", "x")).toEqual({
        kind: "failed",
        message: "",
        state: "hors-ligne",
      });
    } finally {
      if (enLigne === undefined) Reflect.deleteProperty(globalThis, "navigator");
      else Object.defineProperty(globalThis, "navigator", enLigne);
    }
  });
});

describe("un envoi refusé avant toute action est une erreur, pas un « inconnu »", () => {
  test("un 413 du démon : `erreur` (rien n'est parti)", async () => {
    repond(
      JSON.stringify({ error: "text too large (maximum 32768 UTF-8 bytes)" }),
      413,
      "application/json",
    );
    const issue = await sendPrompt("w1:p1", "x");
    expect(issue.kind).toBe("failed");
    expect(issue.kind === "failed" ? issue.state : undefined).toBe("erreur");
  });

  test("un 5xx sans corps nommé reste « inconnu »", async () => {
    repond(JSON.stringify({ error: "boom" }), 500, "application/json");
    const issue = await sendPrompt("w1:p1", "x");
    expect(issue.kind === "failed" ? issue.state : undefined).toBe("inconnu");
  });

  test("trop long : refusé sur le téléphone, AUCUNE requête", async () => {
    let appels = 0;
    globalThis.fetch = Object.assign(
      async () => {
        appels += 1;
        return new Response("{}");
      },
      { preconnect: original.preconnect },
    );
    // 4 octets par emoji : 8 193 emoji dépassent 32 KiB.
    const issue = await sendPrompt("w1:p1", "🙂".repeat(8193));
    expect(appels).toBe(0);
    expect(issue.kind === "failed" ? issue.state : undefined).toBe("erreur");
    expect(issue.kind === "failed" ? issue.message : "").toContain("nothing was sent");
  });
});

/** `fetch` rejette avec `cause` (délai, connexion coupée). */
const leve = (cause: unknown) => {
  globalThis.fetch = Object.assign(
    async () => {
      throw cause;
    },
    { preconnect: original.preconnect },
  );
};

describe("une action sans réponse dit ce qu'on en sait", () => {
  test("une réponse coupée par un délai est « transmise », pas « refusée »", async () => {
    leve(new DOMException("timed out", "TimeoutError"));
    const issue = await answerQuestion("w1:p1", 2, "e1");
    expect(issue.state).toBe("transmise");
    expect(issue.detail).toContain("may have been applied");
  });

  test("une réponse coupée par l'arrière-plan n'affiche pas « Load failed » brut", async () => {
    leve(new TypeError("Load failed"));
    const issue = await answerQuestion("w1:p1", 2, "e1");
    expect(issue.state).toBe("transmise");
    expect(issue.detail).toContain("Connection to the daemon interrupted");
  });

  test("un lancement sans réponse le dit — il a peut-être démarré", async () => {
    leve(new DOMException("timed out", "TimeoutError"));
    const erreur = await launchAgent("sp1", "claude").catch((cause: unknown) => cause);
    expect(erreur instanceof Error ? erreur.message : "").toContain("may have been applied");
  });
});

describe("santé : le 503 est une réponse, pas une panne (H01)", () => {
  test("Herdr injoignable se lit, au lieu de « failed (503) »", async () => {
    repond(
      JSON.stringify({
        herdr: { reachable: false, socket: "/s/herdr.sock" },
        ok: false,
        uptimeMs: 1,
      }),
      503,
      "application/json",
    );
    const sante = await fetchHealth();
    expect(sante.ok).toBe(false);
    expect(sante.herdr.reachable).toBe(false);
  });

  test("un autre échec reste une erreur nommée", async () => {
    repond(JSON.stringify({ error: "boom" }), 500, "application/json");
    await expect(fetchHealth()).rejects.toThrow("boom");
  });
});

describe("une réponse n'est « refusée » que si rien n'est parti (F3.2, B1.1)", () => {
  const json = "application/json";

  test("un 502 du démon est « transmise » : la touche a pu partir", async () => {
    repond(JSON.stringify({ error: "answer failed: send_keys timed out" }), 502, json);
    const issue = await answerQuestion("w1:p1", 1, "e1");
    expect(issue.state).toBe("transmise");
    expect(issue.detail).toContain("check the agent before retrying");
  });

  test("un corps illisible n'est pas un refus", async () => {
    repond("<html>bad gateway</html>", 502, "text/html");
    expect((await answerQuestion("w1:p1", 1, "e1")).state).toBe("transmise");
  });

  test("un 4xx est un refus : rien n'est parti", async () => {
    repond(JSON.stringify({ error: "invalid body" }), 400, json);
    expect((await answerQuestion("w1:p1", 1, "e1")).state).toBe("refusee");
  });

  test("une question changée (409) est refusée avec sa cause", async () => {
    repond(
      JSON.stringify({
        detail: "the question on screen changed since it was shown — nothing was sent",
        state: "refusee",
        status: "question-answered",
      }),
      409,
      json,
    );
    const issue = await answerQuestion("w1:p1", 1, "e1");
    expect(issue.state).toBe("refusee");
    expect(issue.detail).toContain("changed since it was shown");
  });

  test("l'empreinte de la question affichée part avec la réponse", async () => {
    let corps = "";
    globalThis.fetch = Object.assign(
      async (_url: unknown, init?: RequestInit) => {
        corps = typeof init?.body === "string" ? init.body : "";
        return new Response("{}", { status: 400 });
      },
      { preconnect: original.preconnect },
    );
    await answerQuestion("w1:p1", 2, "abc123");
    expect(JSON.parse(corps)).toEqual({ empreinte: "abc123", numero: 2 });
  });

  test("un changement de modèle en 5xx n'est pas « refusé »", async () => {
    repond(JSON.stringify({ error: "model change failed" }), 502, json);
    expect((await changeModel("w1:p1", { model: "opus" })).state).toBe("transmise");
  });
});

describe("un lancement sans verdict n'est pas un échec (F1.1)", () => {
  test("un 502 est une issue inconnue : l'agent a peut-être démarré", async () => {
    repond(JSON.stringify({ error: "launch timed out" }), 502, "application/json");
    const erreur = await launchAgent("sp1", "claude").catch((cause: unknown) => cause);
    expect(erreur).toBeInstanceOf(LancementIncertain);
  });

  test("une coupure après l'envoi est une issue inconnue", async () => {
    leve(new DOMException("timed out", "TimeoutError"));
    const erreur = await launchAgent("sp1", "claude").catch((cause: unknown) => cause);
    expect(erreur).toBeInstanceOf(LancementIncertain);
  });

  test("un 4xx est un vrai refus : rien n'a été lancé", async () => {
    repond(JSON.stringify({ error: "unknown workspace" }), 404, "application/json");
    const erreur = await launchAgent("sp1", "claude").catch((cause: unknown) => cause);
    expect(erreur).toBeInstanceOf(Error);
    expect(erreur).not.toBeInstanceOf(LancementIncertain);
  });
});

describe("chaque action porte sa clé d'idempotence", () => {
  test("deux actions, deux clés : un Retry n'est jamais pris pour une répétition", async () => {
    const cles: string[] = [];
    globalThis.fetch = Object.assign(
      async (_url: unknown, init?: RequestInit) => {
        const entetes = new Headers(init?.headers);
        cles.push(entetes.get("idempotency-key") ?? "");
        return new Response("{}", { status: 400 });
      },
      { preconnect: original.preconnect },
    );
    await answerQuestion("w1:p1", 1, "e1");
    await answerQuestion("w1:p1", 1, "e1");
    expect(cles).toHaveLength(2);
    expect(cles[0]?.length).toBeGreaterThan(0);
    expect(cles[0]).not.toBe(cles[1]);
  });
});
