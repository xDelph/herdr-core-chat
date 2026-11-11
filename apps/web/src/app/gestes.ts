/**
 * Les gestes qui **écrivent** chez un agent — répondre, faire confiance,
 * arrêter, montrer sur le Mac.
 *
 * ⚠️ **Chaque geste capture sa cible à l'appel** (`paneId` passé en argument,
 * jamais relu sur l'écran) et range son issue sous `cleAction(cible, genre,
 * sujet)`. Une issue revenue pour un sujet périmé est rejetée par le magasin
 * d'actions — elle ne peut pas se poser sous une autre question ou un autre
 * agent.
 *
 * ⚠️ **Rien n'est présenté comme fait sans preuve.** « Transmise » n'est pas
 * « appliquée » ; une confiance « transmise » n'est pas « accordée ».
 */
import { answerQuestion, answerTrust, pinAgent, reportError, stopAgent } from "../api.ts";
import { cleAction, type EtatAction } from "../actions-locales.ts";
import { expliquerConfiance } from "../etats-envoi.ts";
import { actions, magasin, notifications } from "./services.ts";

const cause = (erreur: unknown): string =>
  erreur instanceof Error ? erreur.message : String(erreur);

const execute = async (
  cle: string,
  geste: () => Promise<EtatAction>,
  rapport: {
    readonly message: string;
    readonly paneId: string;
    /**
     * L'issue part aussi en notification en haut de l'écran. Pour les gestes
     * sans emplacement propre (arrêter, afficher sur le Mac) : leur issue
     * s'affichait sous le composeur, là où on ne la cherche pas.
     */
    readonly notifie?: boolean;
  },
): Promise<void> => {
  const jeton = actions.debute(cle);
  if (jeton === null) return;
  let issue: EtatAction;
  try {
    issue = await geste();
  } catch (erreur) {
    issue = { phase: "fini", texte: cause(erreur), ton: "erreur" };
  }
  if (issue.phase === "fini" && issue.ton === "erreur") {
    reportError(rapport.message, { detail: issue.texte, paneId: rapport.paneId });
  }
  actions.termine(cle, jeton, issue);
  if (rapport.notifie === true && issue.phase === "fini") {
    notifications.annonce(issue.ton, issue.texte);
  }
  // L'état de l'agent a pu changer : la liste le dira au prochain vol.
  void magasin.rafraichit();
};

export const sujetQuestion = (texte: string): string => texte;

export const repondre = (
  paneId: string,
  question: { readonly texte: string; readonly empreinte?: string | undefined },
  numero: number,
  texte?: string,
): Promise<void> =>
  execute(
    cleAction(paneId, "reponse", sujetQuestion(question.texte)),
    async () => {
      const resultat = await answerQuestion(paneId, numero, question.empreinte, texte);
      switch (resultat.state) {
        case "appliquee":
          return { phase: "fini", texte: `Answer ${String(numero)} applied.`, ton: "ok" };
        case "transmise":
          return {
            phase: "fini",
            texte: `Answer ${String(numero)} sent — Herdr has not confirmed it was applied. ${resultat.detail}`,
            ton: "attention",
          };
        default:
          return { phase: "fini", texte: `Answer rejected: ${resultat.detail}`, ton: "erreur" };
      }
    },
    { message: "failed to answer a question", paneId },
  );

export const faireConfiance = (paneId: string, dossier: string, accepte: boolean): Promise<void> =>
  execute(
    cleAction(paneId, "confiance", dossier),
    async () => {
      const resultat = await answerTrust(paneId, accepte);
      const issue = expliquerConfiance(resultat.state);
      if (resultat.state === "inconnu") {
        return { phase: "fini", texte: `${issue.texte} (${resultat.detail})`, ton: "erreur" };
      }
      return {
        phase: "fini",
        texte: issue.texte,
        ton: issue.resolue ? (resultat.state === "refusee" ? "attention" : "ok") : "attention",
      };
    },
    { message: "failed to answer a trust prompt", paneId },
  );

export const arreter = (paneId: string): Promise<void> =>
  execute(
    cleAction(paneId, "arret"),
    async () => {
      const resultat = await stopAgent(paneId);
      if (resultat.status !== "interrupt-sent") {
        return { phase: "fini", texte: "Herdr reports the agent is no longer working.", ton: "ok" };
      }
      // Ce que herdr-core a vu après la touche (B1.14) — dit tel quel.
      switch (resultat.outcome) {
        case "stopped":
          return { phase: "fini", texte: "Stopped: the agent is no longer working.", ton: "ok" };
        case "exited":
          return { phase: "fini", texte: "Stopped: the agent exited.", ton: "ok" };
        case "still-working":
          return {
            phase: "fini",
            texte: "Interrupt sent, but the agent is still working. Try Stop again.",
            ton: "attention",
          };
        default:
          return {
            phase: "fini",
            texte: "Interrupt sent by Herdr — the agent has not confirmed it stopped yet.",
            ton: "attention",
          };
      }
    },
    { message: "failed to interrupt the agent", notifie: true, paneId },
  );

/**
 * Épingle un agent en tête des listes, ou l'en retire — partagé avec Herdr.app
 * (`herdr.pinned`). ⚠️ Rien d'optimiste : la liste montre l'épingle quand le
 * démon l'a posée (relecture à la fin du geste), et un échec se dit.
 */
export const epingler = (paneId: string, pinned: boolean): Promise<void> =>
  execute(
    cleAction(paneId, "epingle"),
    async () => {
      const resultat = await pinAgent(paneId, pinned);
      return {
        phase: "fini",
        texte: resultat.pinned ? "Pinned to the top, here and in Herdr." : "Unpinned.",
        ton: "ok",
      };
    },
    { message: "failed to pin the agent", notifie: true, paneId },
  );
