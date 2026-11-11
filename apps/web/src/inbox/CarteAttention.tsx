/**
 * Une carte « Attend une décision » — on répond **depuis la liste**.
 *
 * ⚠️ **On n'offre que ce qui est à l'écran de l'agent.** Les choix sont ceux
 * que le serveur a lus (`question.choix`), et il revalide le numéro avant de
 * l'envoyer. L'option de saisie libre ouvre la conversation : son texte se
 * tape dans le panneau de question, avec le choix.
 *
 * ⚠️ **Le détail d'un choix est écrit, pas caché dans un `title`** (invisible
 * au doigt), et un choix qui DURE (« don't ask again ») demande un second
 * appui (F1.5).
 *
 * ⚠️ **La carte suit ce que herdr-core dit de l'attente** : sa nature
 * (permission, question, connexion à finir), ou la fin du programme en
 * erreur avec son code et sa raison — jamais un « waiting » générique quand
 * le core en sait plus.
 */
import { memo, useEffect, useState } from "react";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { estOptionSaisieLibre } from "@herdr-core-chat/shared";
import { cleAction } from "../actions-locales.ts";
import { finDeTerminal, natureAttente } from "../attente.ts";
import { DELAI_CONFIRMATION_MS, estChoixARisque } from "../choix-risque.ts";
import { libelleRefus } from "../etats-envoi.ts";
import { faireConfiance, repondre } from "../app/gestes.ts";
import { useAction } from "../app/hooks.ts";
import { marqueAgent, quand } from "../libelles.ts";
import { routeToHash } from "../routing.ts";
import { Marque } from "../ui/atomes.tsx";
import { IssueAction } from "../ui/IssueAction.tsx";

export const CarteAttention = memo(
  ({
    conversation,
    projet,
    titre,
    maintenant,
  }: {
    readonly conversation: ConversationSummary;
    readonly projet: string;
    readonly titre: string;
    readonly maintenant: number;
  }) => {
    const { paneId, question, trust, waiting } = conversation;
    const nature = natureAttente(conversation);
    const fin = finDeTerminal(conversation);
    const enErreur = conversation.status === "error";
    const connexion = conversation.status === "blocked" && conversation.blockedKind === "auth";
    const lien = routeToHash({ kind: "agent", paneId });
    const cle =
      trust !== undefined
        ? cleAction(paneId, "confiance", trust.folder)
        : cleAction(paneId, "reponse", question?.texte ?? "");
    const issue = useAction(cle);
    const occupe = issue?.phase === "en-cours";
    // Le choix à risque armé par un premier appui — désarmé après un délai.
    const [arme, setArme] = useState<number | null>(null);
    useEffect(() => {
      if (arme === null) return undefined;
      const minuterie = setTimeout(() => setArme(null), DELAI_CONFIRMATION_MS);
      return () => clearTimeout(minuterie);
    }, [arme]);
    // Une autre question : rien ne reste armé.
    const texteQuestion = question?.texte;
    useEffect(() => {
      setArme(null);
    }, [texteQuestion]);

    return (
      <article className="carte" aria-label={`${projet} needs a decision`}>
        <div className="carte__qui">
          <Marque kind={conversation.agent} />
          <span className="carte__projet">{projet}</span>
          <span>{marqueAgent(conversation.agent).nom}</span>
          {conversation.timestamp === undefined ? null : (
            <span className="carte__quand num">{quand(conversation.timestamp, maintenant)}</span>
          )}
        </div>
        {titre.length > 0 && titre !== projet ? <div className="carte__titre">{titre}</div> : null}
        {nature === null ? null : <p className="carte__nature">{nature}</p>}

        {trust !== undefined ? (
          <>
            <p className="carte__q">
              Trust the folder <span className="mono">{trust.folder}</span>?
            </p>
            <p className="discret">
              The agent will be able to read, write and run code there without asking again. This
              choice is saved on the Mac.
            </p>
            <div className="carte__options">
              <button
                type="button"
                className="bouton bouton--principal"
                disabled={occupe}
                onClick={() => void faireConfiance(paneId, trust.folder, true)}
              >
                Trust
              </button>
              <button
                type="button"
                className="bouton"
                disabled={occupe}
                onClick={() => void faireConfiance(paneId, trust.folder, false)}
              >
                {libelleRefus(trust.kind)}
              </button>
            </div>
          </>
        ) : question !== undefined ? (
          <>
            <p className="carte__q">{question.texte}</p>
            {question.contexte.length > 0 ? (
              <p className="discret carte__contexte">{question.contexte.join(" · ")}</p>
            ) : null}
            <div className="carte__options">
              {question.choix.map((choix) =>
                estOptionSaisieLibre(choix.libelle) ? (
                  <a key={choix.numero} className="bouton" href={lien}>
                    <span className="k">{choix.numero}</span>
                    Answer freely…
                  </a>
                ) : (
                  <button
                    key={choix.numero}
                    type="button"
                    className={
                      arme === choix.numero
                        ? "bouton bouton--attn bouton--arme"
                        : choix.selectionne
                          ? "bouton bouton--attn"
                          : "bouton"
                    }
                    disabled={occupe}
                    onClick={() => {
                      if (estChoixARisque(choix.libelle, choix.detail) && arme !== choix.numero) {
                        setArme(choix.numero);
                        return;
                      }
                      setArme(null);
                      void repondre(paneId, question, choix.numero);
                    }}
                  >
                    <span className="k">{choix.numero}</span>
                    <span className="bouton__corps">
                      {arme === choix.numero
                        ? `Tap again to confirm: ${choix.libelle}`
                        : choix.libelle}
                      {choix.detail === undefined ? null : <small>{choix.detail}</small>}
                    </span>
                  </button>
                ),
              )}
              <a className="lien" href={lien}>
                Open
              </a>
            </div>
          </>
        ) : enErreur ? (
          <>
            <p className="carte__q">Stopped with an error</p>
            {fin === null ? null : <p className="discret num">{fin}</p>}
            {conversation.activity === undefined ? null : (
              <pre className="carte__ecran">{conversation.activity}</pre>
            )}
            <div className="carte__options">
              <a className="bouton" href={lien}>
                Open
              </a>
            </div>
          </>
        ) : connexion ? (
          <>
            <p className="carte__q">Sign-in required</p>
            <p className="discret">
              {marqueAgent(conversation.agent).nom} is waiting for authentication. Finish sign-in on
              the Mac.
            </p>
            <div className="carte__options">
              <a className="bouton" href={lien}>
                Open
              </a>
            </div>
          </>
        ) : (
          <>
            <p className="carte__q">
              {waiting === undefined
                ? "The agent is waiting, but its screen shows nothing readable."
                : "The agent is waiting for input — no numbered choices."}
            </p>
            {waiting === undefined ? null : (
              <pre className="carte__ecran">{waiting.split("\n").slice(-6).join("\n")}</pre>
            )}
            <div className="carte__options">
              <a className="bouton" href={lien}>
                Open to answer
              </a>
            </div>
          </>
        )}
        <IssueAction issue={issue} enCours="Sending the answer…" />
      </article>
    );
  },
);
CarteAttention.displayName = "CarteAttention";
