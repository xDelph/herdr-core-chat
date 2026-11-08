/**
 * Une lecture **interrompue** n'est pas une panne.
 *
 * ⚠️ iOS coupe les requêtes en vol quand la PWA passe en arrière-plan : elles
 * reviennent en « Fetch is aborted » (AbortError) au retour. Elles partaient
 * au journal du démon en ERROR et faisaient clignoter « Offline — Daemon
 * unreachable » alors que rien n'était en panne : la relecture du retour au
 * premier plan les remplace.
 */
export const estInterruption = (cause: string): boolean =>
  /fetch is aborted|aborterror|operation was aborted|the user aborted a request|request was aborted|signal is aborted/i.test(
    cause,
  );
