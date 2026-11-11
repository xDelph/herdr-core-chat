/**
 * Ce que herdr-core dit d'une attente et d'une fin, mis en mots — sans rien
 * deviner quand il se tait.
 */
import { describe, expect, test } from "bun:test";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { finDeTerminal, natureAttente } from "../src/attente.ts";
import { CarteAttention } from "../src/inbox/CarteAttention.tsx";

const carte = (morceau: Partial<ConversationSummary>): string =>
  renderToStaticMarkup(
    createElement(CarteAttention, {
      conversation: conversation(morceau),
      maintenant: 0,
      projet: "atelier",
      titre: "",
    }),
  ).replaceAll("&#x27;", "'");

const conversation = (morceau: Partial<ConversationSummary>): ConversationSummary => ({
  agent: "claude",
  name: null,
  paneId: "t1",
  paneRank: null,
  sessionState: "available",
  status: "blocked",
  tabId: "tab",
  tabPaneCount: 1,
  title: "atelier",
  workspaceId: "w1",
  ...morceau,
});

describe("natureAttente", () => {
  test("la nature dite par le core, et seulement elle", () => {
    expect(natureAttente(conversation({ blockedKind: "permission" }))).toBe("Permission needed");
    expect(natureAttente(conversation({ blockedKind: "question" }))).toBe("Question");
    expect(natureAttente(conversation({ blockedKind: "auth" }))).toBe("Sign-in needed");
    expect(natureAttente(conversation({}))).toBeNull();
  });

  test("un agent qui n'attend plus n'a pas de nature d'attente", () => {
    expect(natureAttente(conversation({ blockedKind: "auth", status: "working" }))).toBeNull();
  });
});

describe("finDeTerminal", () => {
  test("chaque raison de herdr-core est expliquée, comme le démon", () => {
    const fin = (exit: NonNullable<ConversationSummary["exit"]>) =>
      finDeTerminal(conversation({ exit }));
    expect(fin({ code: 1, reason: "killed" })).toBe("Killed (code 1)");
    expect(fin({ reason: "signaled", signal: "9" })).toBe("Died from signal 9");
    expect(fin({ reason: "signaled" })).toBe("Died from a signal");
    expect(fin({ reason: "server_stop" })).toBe("Stopped when herdr-core stopped");
    expect(fin({ code: 0, reason: "host_shutdown" })).toBe(
      "Stopped when the Mac shut down (code 0)",
    );
    expect(fin({ reason: "holder_lost" })).toBe("Lost: the process holding it died");
    expect(fin({ reason: "stopped_by_restart" })).toBe(
      "Not running any more after herdr-core restarted",
    );
    expect(fin({ reason: "spawn_pending" })).toBe("Still starting");
    expect(fin({ code: 3, reason: "exited" })).toBe("Exited with code 3");
    expect(fin({ signal: "9" })).toBe("Exited (signal 9)");
    expect(fin({ code: 2, reason: "mystere" })).toBe("Ended (mystere) (code 2)");
    expect(finDeTerminal(conversation({}))).toBeNull();
  });
});

describe("les cartes suivent ce que dit herdr-core", () => {
  test("un programme en erreur : « Stopped with an error », son code et son message", () => {
    const html = carte({
      activity: "Error: rate limit exceeded (429)",
      exit: { code: 1, reason: "killed" },
      status: "error",
    });
    expect(html).toContain("Stopped with an error");
    expect(html).toContain("Killed (code 1)");
    expect(html).toContain("Error: rate limit exceeded (429)");
  });

  test("une connexion à finir : on le dit, sans proposer de choix inventés", () => {
    const html = carte({ blockedKind: "auth" });
    expect(html).toContain("Sign-in needed");
    expect(html).toContain("Sign-in required");
    expect(html).toContain("Finish sign-in on the Mac");
  });

  test("une permission garde ses choix, avec sa nature au-dessus", () => {
    const html = carte({
      blockedKind: "permission",
      question: {
        choix: [{ libelle: "Yes", numero: 1, selectionne: true }],
        contexte: [],
        texte: "Allow this Bash command?",
      },
    });
    expect(html).toContain("Permission needed");
    expect(html).toContain("Allow this Bash command?");
  });
});
