/**
 * Les sous-agents EN DIRECT d'une conversation (lot 4) : les records 7501 que
 * le programme publie (Claude Code en porte un par sous-agent lancé).
 *
 * ⚠️ **Ce que le programme dit, maintenant** — son état, son titre, son
 * message, dans ses propres mots. Il passe avant l'heuristique des fichiers
 * (« en cours » seulement si actif depuis moins de 30 min) : c'est l'état
 * déclaré. Absent, rien n'est montré, rien n'est deviné.
 */
import { memo } from "react";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { Etat } from "../ui/atomes.tsx";

export type SousAgent = NonNullable<ConversationSummary["subagents"]>[number];

export const SousAgents = memo(({ liste }: { readonly liste: ReadonlyArray<SousAgent> }) => {
  if (liste.length === 0) return null;
  return (
    <section className="sous-agents" aria-label="Live sub-agents">
      <h3 className="sous-agents__titre">
        {liste.length === 1 ? "1 sub-agent" : `${String(liste.length)} sub-agents`}
      </h3>
      <ul>
        {liste.map((sous) => (
          <li key={sous.id} className="sous-agents__ligne">
            <Etat status={sous.status}>
              <span className="sous-agents__nom">{sous.title ?? sous.id}</span>
            </Etat>
            {sous.msg === undefined ? null : <span className="sous-agents__msg">{sous.msg}</span>}
          </li>
        ))}
      </ul>
    </section>
  );
});
SousAgents.displayName = "SousAgents";
