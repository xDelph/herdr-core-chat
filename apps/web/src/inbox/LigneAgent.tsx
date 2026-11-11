/**
 * Une ligne de la liste (maquette « Inbox », 2026-11-10) : **deux lignes au
 * plus**, le logo de l'agent en tête et son état en badge dessus.
 *
 * ```
 * [logo+badge]  Titre                         6:28 | 1 h
 *               projet · ce qui change
 * ```
 *
 * - **au travail** : le chronomètre depuis la demande qui a ouvert le tour,
 *   lue dans la session (`tourDebut`, la même origine que le chat) ; à défaut,
 *   depuis le passage à `working` *observé* ; sinon pas de durée inventée.
 *   La seconde ligne dit ce que l'agent dit faire (`activity`, `status.msg`
 *   de herdr-core, dans ses propres mots), à défaut la demande ;
 * - **en attente de ses sous-agents** : « Waiting on 2 subagents » ;
 * - **des commandes en fond** (serveur de dev…) : une icône de terminal et
 *   leur nombre à côté du titre, sans chronomètre ([`IndiceFond`]) ;
 * - **sinon** : la demande qui a lancé le travail (retour du 2026-10-29), le
 *   temps écoulé, et le point « non lu » tant qu'on ne l'a pas ouverte ici.
 *
 * ⚠️ **Branche, modèle et réflexion ne sont plus sur la ligne** (retour du
 * 2026-11-10 : « develop · claude-opus » répété sur 18 lignes ne disait
 * rien). Ils vivent dans l'en-tête de la conversation.
 */
import { memo } from "react";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { apercuTexte, chrono, quand } from "../libelles.ts";
import { attenteDeTaches, commandesEnFond, libelleFond } from "../taches.ts";
import { routeToHash } from "../routing.ts";
import { AvatarAgent, type Badge } from "../ui/atomes.tsx";
import { Icone } from "../ui/Icone.tsx";

/**
 * La demande qui a lancé le travail — toujours elle, quand elle existe : ni un
 * message intermédiaire de l'agent, ni sa dernière commande (retour du
 * 2026-10-29). L'aperçu de l'agent ne sert qu'à défaut de demande.
 */
const apercu = (conversation: ConversationSummary): string =>
  apercuTexte(conversation.lastPrompt ?? conversation.preview ?? "");

/**
 * L'indice d'un agent qui a fini mais dont des commandes tournent en fond :
 * une icône de terminal et leur nombre, sans chronomètre — l'agent, lui, ne
 * travaille pas.
 */
export const IndiceFond = ({ conversation }: { readonly conversation: ConversationSummary }) => {
  const libelle = libelleFond(conversation.tachesDeFond);
  if (libelle === null || conversation.status === "working") return null;
  return (
    <span className="indice-fond" title={libelle}>
      <Icone nom="term" petite />
      <span className="num" aria-hidden="true">
        {commandesEnFond(conversation.tachesDeFond)}
      </span>
      <span className="sr">{libelle}</span>
    </span>
  );
};

/** Le badge d'une ligne qui n'est pas « à toi » (celles-là sont des cartes). */
export const badgeDeLigne = (conversation: ConversationSummary, nonLue: boolean): Badge | null => {
  if (conversation.status === "blocked") return "question";
  if (conversation.status === "error") return "erreur";
  if (conversation.status === "working" || attenteDeTaches(conversation.tachesDeFond) !== null)
    return "travail";
  return nonLue ? "non-lu" : null;
};

export const LigneAgent = memo(
  ({
    conversation,
    projet,
    titre,
    maintenant,
    nonLue,
    debutTravail,
    courante,
    avecProjet = true,
  }: {
    readonly conversation: ConversationSummary;
    readonly projet: string;
    readonly titre: string;
    readonly maintenant: number;
    readonly nonLue: boolean;
    readonly debutTravail: number | undefined;
    readonly courante: boolean;
    /** Dans le détail d'un projet, son nom est le titre de l'écran : la ligne le tait. */
    readonly avecProjet?: boolean;
  }) => {
    const auTravail = conversation.status === "working";
    // La session d'abord : le chat et la liste partent du même instant.
    const debut = conversation.tourDebut ?? debutTravail;
    const attente = auTravail ? null : attenteDeTaches(conversation.tachesDeFond);
    // Un titre vide ou égal au projet : le projet devient le titre, une fois.
    const titreSeul = titre.length > 0 && titre !== projet;
    const intitule = titreSeul ? titre : projet;
    const montreProjet = avecProjet && titreSeul;

    const demande = apercu(conversation);
    const texte =
      auTravail && conversation.activity !== undefined
        ? conversation.activity
        : attente !== null
          ? attente
          : demande;
    const absent =
      texte.length === 0 && conversation.sessionState !== "available"
        ? conversation.sessionState === "absent"
          ? "No readable session for this agent"
          : "Unreadable session"
        : null;

    const classes = ["ligne"];
    if (nonLue) classes.push("ligne--non-lue");
    return (
      <a
        className={classes.join(" ")}
        href={routeToHash({ kind: "agent", paneId: conversation.paneId })}
        aria-current={courante ? "page" : undefined}
      >
        <AvatarAgent kind={conversation.agent} badge={badgeDeLigne(conversation, nonLue)} />
        <span className="ligne__corps">
          <span className="ligne__l1">
            {conversation.pinned === true ? (
              <span className="ligne__epingle" title="Pinned">
                <Icone nom="pin" petite />
                <span className="sr">Pinned: </span>
              </span>
            ) : null}
            <span className="ligne__titre">{intitule}</span>
            <IndiceFond conversation={conversation} />
            {auTravail ? (
              <span className="ligne__quand ligne__quand--travail num">
                {debut === undefined ? "Working" : chrono(maintenant - debut)}
              </span>
            ) : conversation.timestamp === undefined ? null : (
              <span className="ligne__quand num">{quand(conversation.timestamp, maintenant)}</span>
            )}
          </span>
          {texte.length > 0 || absent !== null || montreProjet ? (
            <span
              className={
                absent !== null
                  ? "ligne__l2 ligne__l2--absent"
                  : auTravail
                    ? "ligne__l2 ligne__l2--travail"
                    : "ligne__l2"
              }
            >
              {montreProjet ? <span className="ligne__projet">{projet}</span> : null}
              {montreProjet && (texte.length > 0 || absent !== null) ? " · " : null}
              {auTravail && conversation.activity !== undefined ? (
                <span className="sr">Now: </span>
              ) : null}
              {absent ?? texte}
            </span>
          ) : null}
        </span>
      </a>
    );
  },
);
LigneAgent.displayName = "LigneAgent";

/**
 * Une ligne d'agent **au repos**, sous le pli « N idle » : une seule ligne,
 * petit logo, titre, projet · temps. Ce qui n'attend rien ne prend pas plus
 * de place.
 */
export const LigneRepos = memo(
  ({
    conversation,
    projet,
    titre,
    maintenant,
    courante,
    avecProjet = true,
  }: {
    readonly conversation: ConversationSummary;
    readonly projet: string;
    readonly titre: string;
    readonly maintenant: number;
    readonly courante: boolean;
    readonly avecProjet?: boolean;
  }) => {
    const titreSeul = titre.length > 0 && titre !== projet;
    // Le projet se tronque, le temps jamais : c'est lui qu'on lit d'abord.
    const montreProjet = avecProjet && titreSeul;
    const temps =
      conversation.timestamp === undefined ? null : quand(conversation.timestamp, maintenant);
    return (
      <a
        className="ligne-repos"
        href={routeToHash({ kind: "agent", paneId: conversation.paneId })}
        aria-current={courante ? "page" : undefined}
      >
        <AvatarAgent kind={conversation.agent} badge={null} taille="petite" />
        <span className="ligne-repos__titre">
          {conversation.pinned === true ? (
            <span className="ligne__epingle" title="Pinned">
              <Icone nom="pin" petite />
              <span className="sr">Pinned: </span>
            </span>
          ) : null}
          {titreSeul ? titre : projet}
        </span>
        {montreProjet ? <span className="ligne-repos__projet">{projet}</span> : null}
        <IndiceFond conversation={conversation} />
        {temps === null ? null : <span className="ligne-repos__quand num">{temps}</span>}
      </a>
    );
  },
);
LigneRepos.displayName = "LigneRepos";
