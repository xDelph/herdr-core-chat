/**
 * Ce que herdr-core dit d'un agent qui demande l'utilisateur — mis en mots.
 *
 * ⚠️ **Rien n'est deviné.** La nature d'une attente vient de `status.kind`
 * (`permission | question | auth`), la fin d'un terminal de `exit_code`,
 * `reason` et `signal`. Absents, ils ne sont pas remplacés : la carte dit
 * seulement ce que le core a dit.
 */
import type { ConversationSummary } from "@herdr-core-chat/shared";

/** L'étiquette de la nature d'une attente ; `null` quand le core ne la dit pas. */
export const natureAttente = (conversation: ConversationSummary): string | null => {
  if (conversation.status !== "blocked") return null;
  switch (conversation.blockedKind) {
    case "permission":
      return "Permission needed";
    case "question":
      return "Question";
    case "auth":
      return "Sign-in needed";
    default:
      return null;
  }
};

/**
 * ⚠️ **La fin expliquée (lot 4)**, la même table que le démon
 * (`AgentExit::explication`, `crates/domain/src/status.rs`) : la raison de
 * herdr-core 0.1.19 (`reason`) dite en mots d'utilisateur. Un mot inconnu est
 * montré tel quel, jamais traduit au hasard. `null` : le terminal n'est pas fini.
 */
export const finDeTerminal = (conversation: ConversationSummary): string | null => {
  const fin = conversation.exit;
  if (fin === undefined) return null;
  const code = fin.code === undefined ? "" : ` (code ${String(fin.code)})`;
  switch (fin.reason) {
    case "killed":
      return `Killed${code}`;
    case "signaled":
      return fin.signal === undefined ? "Died from a signal" : `Died from signal ${fin.signal}`;
    case "server_stop":
      return `Stopped when herdr-core stopped${code}`;
    case "host_shutdown":
      return `Stopped when the Mac shut down${code}`;
    case "holder_lost":
      return "Lost: the process holding it died";
    case "stopped_by_restart":
      return "Not running any more after herdr-core restarted";
    case "spawn_pending":
      return "Still starting";
    case undefined:
    case "exited": {
      const sortie = fin.code === undefined ? "Exited" : `Exited with code ${String(fin.code)}`;
      return fin.signal === undefined ? sortie : `${sortie} (signal ${fin.signal})`;
    }
    default:
      return `Ended (${fin.reason})${code}`;
  }
};
