/**
 * La boîte de réception (maquette « Inbox », 2026-11-10) : ce qui attend une
 * décision passe devant, avec ses réponses à portée de pouce ; le reste se lit
 * en deux lignes ; ce qui n'attend rien est replié.
 *
 * ```
 * Needs you     cartes répondables (question, confiance, saisie, erreur)
 * Working       qui travaille, depuis quand, ce qu'il dit faire
 * Unread        réponses finies pas encore ouvertes — « Mark all read »
 * N idle  ›     replié : une ligne par agent une fois déplié
 * ```
 *
 * ⚠️ **Plus de barre d'outils au-dessus de la liste** (retour du 2026-11-10 :
 * regroupement + tri + compte + recherche s'empilaient avant le premier
 * agent). Le regroupement et l'ordre vivent dans les réglages ; la recherche
 * s'ouvre depuis l'en-tête, avec son filtre de projet en liste déroulante ;
 * « lancer » est le bouton flottant.
 */
import { memo, useDeferredValue, useMemo, useState, type ReactNode } from "react";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { useEnLigne, useMaintenant, useTousEnvois } from "../app/hooks.ts";
import { entreesBoite, resumeBoite } from "../boite-envoi.ts";
import { etatConnexion, LIBELLE_CONNEXION } from "../connexion.ts";
import type { EtatDonnees } from "../donnees.ts";
import { etatLecture, FENETRE_DEMARRAGE_MS, heureLue } from "../etats.ts";
import { estNonLue, type Lectures } from "../lectures.ts";
import { lignesAffichees } from "../lignes-provisoires.ts";
import { commandesEnFond } from "../taches.ts";
import { dossierDe, marqueAgent, premierNonVide, titrePropre } from "../libelles.ts";
import type { Groupement, OrdreListe } from "../preferences.ts";
import { routeToHash } from "../routing.ts";
import { groupesDe, SECTIONS, type Groupe, type Section } from "../sections.ts";
import { Avis, Marque } from "../ui/atomes.tsx";
import { Icone } from "../ui/Icone.tsx";
import { SqueletteListe } from "../ui/Squelettes.tsx";
import { CarteAttention } from "./CarteAttention.tsx";
import { EnTeteProjet } from "./EnTeteProjet.tsx";
import { LigneAgent, LigneRepos } from "./LigneAgent.tsx";

const OUVERTURE = Date.now();

/** Combien de logos le pli « N idle » montre en aperçu. */
const LOGOS_DU_PLI = 4;

/** Le texte où chercher : ce que la ligne affiche déjà, rien de plus. */
const botteDe = (conversation: ConversationSummary, projet: string): string =>
  [
    projet,
    conversation.title,
    marqueAgent(conversation.agent).nom,
    conversation.lastPrompt,
    conversation.preview,
    conversation.model,
    conversation.branch,
  ]
    .filter((morceau) => morceau !== undefined && morceau !== null)
    .join(" ")
    .toLowerCase();

/**
 * L'indicateur, ouvrant le centre de connexion quand il est fourni (lot 4) :
 * un mot ne suffit pas à dire QUEL maillon a lâché.
 */
const OuvreConnexion = ({
  onConnexion,
  children,
}: {
  readonly onConnexion: (() => void) | undefined;
  readonly children: ReactNode;
}) =>
  onConnexion === undefined ? (
    children
  ) : (
    <button
      type="button"
      className="direct__bouton"
      aria-label="Connection details"
      onClick={onConnexion}
    >
      {children}
    </button>
  );

/** La pastille de la boîte d'envoi : présente seulement quand elle n'est pas vide. */
const PastilleEnvois = ({ onOuvrir }: { readonly onOuvrir: () => void }) => {
  const resume = resumeBoite(entreesBoite(useTousEnvois()));
  if (resume.total === 0) return null;
  return (
    <button
      type="button"
      className={resume.aRegarder > 0 ? "pastille-envois pastille-envois--attn" : "pastille-envois"}
      aria-label={`Outbox: ${String(resume.total)} message(s) not settled`}
      onClick={onOuvrir}
    >
      <Icone nom="send" />
      <span className="pastille-envois__n num">{resume.total}</span>
    </button>
  );
};

/** L'indicateur de connexion seul — repris par le mur live. */
export const DirectSeul = memo(
  ({
    donnees,
    maintenant,
    enLigne,
  }: {
    readonly donnees: EtatDonnees;
    readonly maintenant: number;
    readonly enLigne: boolean;
  }) => {
    // Le démon répond, mais Herdr non : ce n'est ni « Connecting » ni le
    // réseau — c'est Herdr, et on le nomme.
    if (enLigne && donnees.health?.herdr.reachable === false) {
      return (
        <span className="direct direct--coupe">
          <span className="point" aria-hidden="true" />
          Herdr offline
        </span>
      );
    }
    const etat = etatConnexion({
      dernierSucces: donnees.dernierSucces,
      depuisOuverture: maintenant - OUVERTURE,
      direct: donnees.direct,
      enLigne,
      erreur: donnees.erreurs.conversations,
    });
    if (etat.phase === "direct") {
      return (
        <span className="direct">
          <span className="point" aria-hidden="true" />
          {LIBELLE_CONNEXION.direct}
        </span>
      );
    }
    return (
      <span
        className={etat.phase === "hors-ligne" ? "direct direct--coupe" : "direct direct--attente"}
      >
        <span className="point" aria-hidden="true" />
        {LIBELLE_CONNEXION[etat.phase]}
      </span>
    );
  },
);
DirectSeul.displayName = "DirectSeul";

const Direct = (proprietes: {
  readonly donnees: EtatDonnees;
  readonly maintenant: number;
  readonly enLigne: boolean;
  readonly onConnexion: (() => void) | undefined;
}) => (
  <OuvreConnexion onConnexion={proprietes.onConnexion}>
    <DirectSeul
      donnees={proprietes.donnees}
      maintenant={proprietes.maintenant}
      enLigne={proprietes.enLigne}
    />
  </OuvreConnexion>
);

interface ProprietesBoite {
  readonly donnees: EtatDonnees;
  readonly lectures: Lectures;
  /** Choisi dans les réglages ; dans un projet, toujours par état. */
  readonly groupement: Groupement;
  /** Choisi dans les réglages. */
  readonly ordre: OrdreListe;
  /** `#/projet/<id>` : la liste filtrée sur ce projet, avec « Nouvel agent ». */
  readonly projet?: string | null;
  /** Le filtre d'état choisi dans le rail du bureau (`Rail.tsx`), ou `null`. */
  readonly filtreSection?: Section | null;
  readonly onLancer: () => void;
  readonly onReglages: () => void;
  /** Marque lues les réponses finies données (« Mark all read »). */
  readonly onToutLire?: (lignes: ReadonlyArray<ConversationSummary>) => void;
  /** Ouvre le centre de connexion depuis l'indicateur (lot 4). */
  readonly onConnexion?: () => void;
  /** Ouvre la boîte d'envoi depuis sa pastille (lot 4). */
  readonly onBoiteEnvoi?: () => void;
  /** Relire tout de suite (liste en erreur ou hors ligne). */
  readonly onReessayer: () => void;
  readonly paneCourant: string | null;
  /**
   * La liste est cachée (téléphone, conversation ouverte) : ses horloges se
   * taisent. Elle se redessinait chaque seconde derrière le chat.
   */
  readonly cachee?: boolean;
}

/** Le pli « N idle » : un compte, les premiers logos, et une flèche. */
const PliRepos = ({
  conversations,
  ouvert,
  onBascule,
}: {
  readonly conversations: ReadonlyArray<ConversationSummary>;
  readonly ouvert: boolean;
  readonly onBascule: () => void;
}) => (
  <button type="button" className="pli" aria-expanded={ouvert} onClick={onBascule}>
    <span className="anneau" aria-hidden="true" />
    <span className="pli__libelle">{conversations.length} idle</span>
    <span className="pli__logos" aria-hidden="true">
      {conversations.slice(0, LOGOS_DU_PLI).map((conversation) => (
        <Marque key={conversation.paneId} kind={conversation.agent} />
      ))}
    </span>
    <Icone nom="chev" petite />
  </button>
);

const BoiteVisible = ({
  donnees,
  lectures,
  groupement,
  ordre,
  projet = null,
  filtreSection = null,
  onLancer,
  onReglages,
  onToutLire,
  onConnexion,
  onBoiteEnvoi,
  onReessayer,
  paneCourant,
  cachee = false,
}: ProprietesBoite) => {
  const [recherche, setRecherche] = useState("");
  const [rechercheOuverte, setRechercheOuverte] = useState(false);
  /** Le projet choisi dans la liste déroulante de la recherche ("" : tous). */
  const [projetFiltre, setProjetFiltre] = useState("");
  const [reposOuvert, setReposOuvert] = useState(false);
  const rechercheDifferee = useDeferredValue(recherche);
  const { overview, debutsTravail } = donnees;
  // Avant la liste lue, les lignes de l'overview (`lignes-provisoires.ts`).
  const { lignes: conversations, provisoires } = useMemo(
    () => lignesAffichees(donnees.conversations, overview),
    [donnees.conversations, overview],
  );

  // L'horloge bat à la seconde seulement si un chronomètre est visible.
  const chronometres = (conversations ?? []).some(
    (conversation) =>
      conversation.status === "working" &&
      (conversation.tourDebut !== undefined || debutsTravail.has(conversation.paneId)),
  );
  // ⚠️ Et tant qu'aucune lecture n'a réussi : la fenêtre de démarrage (5 s)
  // doit se FERMER à l'heure. À la demi-minute, « Connecting to the daemon… »
  // restait affiché 30 s alors que la vraie cause était déjà connue.
  const maintenant = useMaintenant(
    cachee ? null : chronometres || donnees.dernierSucces === null ? 1_000 : 30_000,
  );
  const enLigne = useEnLigne();
  // Les temps relatifs (« 25 min ») n'ont pas besoin de la seconde : les lignes
  // qui ne portent pas de chronomètre ne se redessinent qu'à la demi-minute.
  const demiMinute = Math.floor(maintenant / 30_000) * 30_000;

  const projets = useMemo(() => {
    const noms = new Map<string, string>();
    for (const espace of overview?.workspaces ?? []) {
      noms.set(
        espace.workspaceId,
        premierNonVide(espace.label, dossierDe(espace.checkoutPath), espace.workspaceId),
      );
    }
    return noms;
  }, [overview]);
  const nomDuProjet = useMemo(
    () => (workspaceId: string) => projets.get(workspaceId) ?? workspaceId,
    [projets],
  );

  const duProjet = useMemo(
    () =>
      projet === null
        ? (conversations ?? [])
        : (conversations ?? []).filter((conversation) => conversation.workspaceId === projet),
    [conversations, projet],
  );

  const motif = rechercheDifferee.trim().toLowerCase();
  // La liste déroulante ne vaut que recherche ouverte : refermée, tout revient.
  const filtreProjet = rechercheOuverte && projet === null ? projetFiltre : "";
  const filtrees = useMemo(
    () =>
      duProjet.filter(
        (conversation) =>
          (filtreProjet.length === 0 || conversation.workspaceId === filtreProjet) &&
          (motif.length === 0 ||
            botteDe(conversation, nomDuProjet(conversation.workspaceId)).includes(motif)),
      ),
    [duProjet, filtreProjet, motif, nomDuProjet],
  );
  // Le filtre du rail ne vaut que sur la liste entière (un projet a son écran).
  const section = projet === null ? filtreSection : null;
  const filtre = motif.length > 0 || filtreProjet.length > 0 || section !== null;

  // Dans un projet, regrouper par projet n'aurait qu'un groupe : par état. Un
  // filtre d'état aussi : ses lignes sont toutes dans le même.
  const groupes: ReadonlyArray<Groupe> = useMemo(() => {
    const tous = groupesDe(
      filtrees,
      ordre,
      projet === null && section === null ? groupement : "statut",
      nomDuProjet,
      lectures,
    );
    return section === null ? tous : tous.filter((groupe) => groupe.cle === section);
  }, [filtrees, groupement, nomDuProjet, ordre, projet, lectures, section]);

  // Les projets de la liste déroulante : ceux d'Herdr, par nom.
  const choixProjets = useMemo(
    () =>
      [...projets.entries()].toSorted((a, b) =>
        a[1].localeCompare(b[1], "en", { sensitivity: "base" }),
      ),
    [projets],
  );

  const etat = etatLecture({
    chargement: donnees.chargement,
    depuisOuverture: maintenant - OUVERTURE,
    dernierSucces: donnees.dernierSucces,
    erreur: donnees.erreurs.conversations,
    live: donnees.direct === "live",
    // ⚠️ Des lignes provisoires ne sont pas une liste lue : si sa lecture a
    // échoué, l'échec se dit (« Unreadable list »), pas « Offline ».
    total: provisoires && donnees.erreurs.conversations !== null ? 0 : (conversations?.length ?? 0),
  });
  const total = duProjet.length;
  /** Le démon répond, Herdr non — une seule explication remplace les autres. */
  const herdrInjoignable = donnees.health?.herdr.reachable === false;

  const fermeRecherche = () => {
    setRecherche("");
    setProjetFiltre("");
    setRechercheOuverte(false);
  };

  const ligne = (conversation: ConversationSummary) => (
    <LigneAgent
      key={conversation.paneId}
      conversation={conversation}
      projet={nomDuProjet(conversation.workspaceId)}
      titre={titrePropre(conversation.title)}
      maintenant={conversation.status === "working" ? maintenant : demiMinute}
      nonLue={estNonLue(conversation, lectures)}
      debutTravail={debutsTravail.get(conversation.paneId)}
      courante={conversation.paneId === paneCourant}
      avecProjet={projet === null}
    />
  );

  return (
    <section className="boite" aria-label="Agents">
      <header className="boite__haut">
        <div className="boite__titre">
          <h1>Agents</h1>
          <span className="boite__hote">
            <Icone nom="mac" petite />
            <Direct
              donnees={donnees}
              maintenant={maintenant}
              enLigne={enLigne}
              onConnexion={onConnexion}
            />
          </span>
        </div>
        {onBoiteEnvoi === undefined ? null : <PastilleEnvois onOuvrir={onBoiteEnvoi} />}
        <button
          type="button"
          className="bouton-icone"
          aria-label="Search"
          aria-expanded={rechercheOuverte}
          onClick={() => (rechercheOuverte ? fermeRecherche() : setRechercheOuverte(true))}
        >
          <Icone nom="search" />
        </button>
        {/* Le mur live, à un geste de la liste (retour du 2026-11-11 : caché dans les réglages). */}
        <a
          className="bouton-icone boite__mur"
          href={routeToHash({ kind: "mur" })}
          aria-label="Live wall"
        >
          <Icone nom="tuiles" />
        </a>
        <button type="button" className="bouton-icone" aria-label="Settings" onClick={onReglages}>
          <Icone nom="sliders" />
        </button>
      </header>
      {projet === null ? null : (
        <EnTeteProjet
          nom={nomDuProjet(projet)}
          existe={projets.has(projet)}
          lu={overview !== null}
          onLancer={onLancer}
        />
      )}
      {rechercheOuverte ? (
        <div className="recherche">
          <div className="recherche__ligne">
            <label className="recherche__champ">
              <Icone nom="search" petite />
              <span className="sr">Search</span>
              <input
                type="search"
                value={recherche}
                placeholder="Title, project, agent, message…"
                // oxlint-disable-next-line jsx-a11y/no-autofocus -- ouverte par un geste explicite : le clavier est attendu
                autoFocus
                onChange={(evenement) => setRecherche(evenement.target.value)}
              />
            </label>
            <button type="button" className="lien recherche__annuler" onClick={fermeRecherche}>
              Cancel
            </button>
          </div>
          <div className="recherche__filtres">
            {projet === null ? (
              <label className="deroulant">
                <span className="sr">Project</span>
                <select
                  value={projetFiltre}
                  onChange={(evenement) => setProjetFiltre(evenement.target.value)}
                >
                  <option value="">All projects</option>
                  {choixProjets.map(([id, nom]) => (
                    <option key={id} value={id}>
                      {nom}
                    </option>
                  ))}
                </select>
                <Icone nom="chev" petite />
              </label>
            ) : null}
            {/* Tant que la liste n'est pas lue, le compte n'est pas « 0 » : il est inconnu. */}
            {conversations === null ? null : (
              <span className="discret num">
                {filtre
                  ? `${String(filtrees.length)} of ${String(total)}`
                  : `${String(total)} agent${total === 1 ? "" : "s"}`}
              </span>
            )}
          </div>
        </div>
      ) : null}

      <div className="boite__liste" aria-busy={provisoires ? true : undefined}>
        {herdrInjoignable ? (
          <Avis ton="attention" annonce="statut">
            <div className="avis__corps">
              <p>
                <strong>Can't reach herdr-core on the Mac.</strong> Start it there (
                <code>hc server start</code>) — this list fills in by itself once it answers.
              </p>
              {donnees.erreurs.overview === null ? null : (
                <p className="avis__detail">Cause: {donnees.erreurs.overview}</p>
              )}
              {donnees.health?.herdr.socket === undefined ? null : (
                <p className="avis__detail mono">{donnees.health.herdr.socket}</p>
              )}
              <p>
                <button
                  type="button"
                  className="lien lien--avis"
                  disabled={donnees.chargement}
                  onClick={onReessayer}
                >
                  {donnees.chargement ? "Checking…" : "Check again"}
                </button>
              </p>
            </div>
          </Avis>
        ) : null}
        {etat.phase === "hors-ligne" && !herdrInjoignable ? (
          <Avis ton="attention" annonce="statut">
            Offline — last read at {heureLue(etat.dernierSucces) ?? "?"} ·{" "}
            {donnees.erreurs.conversations}{" "}
            <button
              type="button"
              className="lien lien--avis"
              disabled={donnees.chargement}
              onClick={onReessayer}
            >
              {donnees.chargement ? "Loading…" : "Retry"}
            </button>
          </Avis>
        ) : null}
        {etat.phase === "erreur" && !herdrInjoignable ? (
          <Avis ton="erreur" annonce="alerte">
            Unreadable list: {etat.cause}{" "}
            <button
              type="button"
              className="lien lien--avis"
              disabled={donnees.chargement}
              onClick={onReessayer}
            >
              {donnees.chargement ? "Loading…" : "Retry"}
            </button>
          </Avis>
        ) : null}
        {donnees.erreurs.overview !== null && etat.phase !== "erreur" && !herdrInjoignable ? (
          <Avis ton="attention">Unreadable projects: {donnees.erreurs.overview}</Avis>
        ) : null}
        {/* Des lignes provisoires sont déjà la forme de la liste : pas de squelette en plus. */}
        {etat.phase === "chargement" && !herdrInjoignable && !provisoires ? (
          <SqueletteListe
            libelle={
              maintenant - OUVERTURE < FENETRE_DEMARRAGE_MS
                ? "Connecting to the daemon…"
                : "Loading agents…"
            }
          />
        ) : null}
        {etat.phase === "vide" ? (
          <p className="boite__vide">No agent open in Herdr. Launch one with +.</p>
        ) : null}
        {projet !== null && etat.phase !== "vide" && total === 0 && conversations !== null ? (
          <p className="boite__vide">No agent in this project. Launch one with “New agent”.</p>
        ) : null}
        {section !== null && groupes.length === 0 && conversations !== null ? (
          <p className="boite__vide">No agent here right now.</p>
        ) : null}
        {total > 0 && filtrees.length === 0 ? (
          <p className="boite__vide">
            {motif.length > 0 ? `Nothing matches “${recherche.trim()}”.` : "No agent here."}
          </p>
        ) : null}

        {groupes.map((groupe) => {
          const estSection = (SECTIONS as ReadonlyArray<string>).includes(groupe.cle);
          /*
            Le repos se replie, mais seulement sous autre chose :
            - quand on cherche, un résultat caché derrière un pli ne serait pas
              un résultat ;
            - seul groupe de la liste (soirée calme, détail d'un projet), le
              plier laisserait un écran vide derrière « 18 idle » ;
            - lignes provisoires (démarrage à froid) : leur état n'est pas
              encore lu, elles ne sont pas « au repos ».
          */
          const repli = groupe.cle === "idle" && !filtre && !provisoires && groupes.length > 1;
          if (repli) {
            /*
              ⚠️ Un agent au repos dont des commandes tournent en fond reste
              VISIBLE au-dessus du pli (retour du 2026-11-11) : son indice
              caché sous « N idle » ne servirait à rien.
            */
            const enFond = groupe.conversations.filter(
              (conversation) => commandesEnFond(conversation.tachesDeFond) > 0,
            );
            const calmes = groupe.conversations.filter(
              (conversation) => commandesEnFond(conversation.tachesDeFond) === 0,
            );
            const ligneRepos = (conversation: ConversationSummary) => (
              <LigneRepos
                key={conversation.paneId}
                conversation={conversation}
                projet={nomDuProjet(conversation.workspaceId)}
                titre={titrePropre(conversation.title)}
                maintenant={demiMinute}
                courante={conversation.paneId === paneCourant}
                avecProjet={projet === null}
              />
            );
            return (
              <section key={groupe.cle} className="groupe groupe--repos" aria-label="Idle">
                {enFond.length > 0 ? (
                  <div className="lignes-repos">{enFond.map(ligneRepos)}</div>
                ) : null}
                {calmes.length === 0 ? null : (
                  <PliRepos
                    conversations={calmes}
                    ouvert={reposOuvert}
                    onBascule={() => setReposOuvert((valeur) => !valeur)}
                  />
                )}
                {reposOuvert && calmes.length > 0 ? (
                  <div className="lignes-repos">{calmes.map(ligneRepos)}</div>
                ) : null}
              </section>
            );
          }
          return (
            <section
              key={groupe.cle}
              className={`groupe groupe--${estSection ? groupe.cle : "projet"}`}
              aria-labelledby={`groupe-${groupe.cle}`}
            >
              <div className="groupe__tete">
                <h2 id={`groupe-${groupe.cle}`}>
                  {/* Un groupe de projet mène au détail du projet. */}
                  {groupement === "projet" && projet === null && !estSection ? (
                    <a href={routeToHash({ kind: "projet", workspaceId: groupe.cle })}>
                      {groupe.titre}
                    </a>
                  ) : (
                    groupe.titre
                  )}
                </h2>
                <span className="groupe__n num">{groupe.conversations.length}</span>
                {groupe.cle === "done" && onToutLire !== undefined ? (
                  <button
                    type="button"
                    className="lien groupe__action"
                    onClick={() => onToutLire(groupe.conversations)}
                  >
                    Mark all read
                  </button>
                ) : null}
              </div>
              {groupe.cle === "a-traiter" ? (
                groupe.conversations.map((conversation) => (
                  <CarteAttention
                    key={conversation.paneId}
                    conversation={conversation}
                    projet={nomDuProjet(conversation.workspaceId)}
                    titre={titrePropre(conversation.title)}
                    maintenant={demiMinute}
                  />
                ))
              ) : (
                <div className="lignes">{groupe.conversations.map(ligne)}</div>
              )}
            </section>
          );
        })}
        <div className="boite__fin" />
      </div>
      <button type="button" className="fab" aria-label="Launch an agent" onClick={onLancer}>
        <Icone nom="plus" />
      </button>
    </section>
  );
};

/**
 * ⚠️ **Une liste cachée ne se redessine pas** (audit, performance). Sur
 * téléphone, conversation ouverte, la boîte est hors de l'écran : chaque
 * rafraîchissement la recalculait quand même (groupes, tri, lignes). Ici elle
 * reste figée tant qu'elle est cachée, et se redessine en revenant à l'écran
 * — avec les données du moment. Visible, elle ne se redessine que si une
 * propriété a changé.
 */
const memesProprietes = (avant: ProprietesBoite, apres: ProprietesBoite): boolean => {
  if (avant.cachee === true && apres.cachee === true) return true;
  const cles = new Set([...Object.keys(avant), ...Object.keys(apres)]);
  for (const cle of cles) {
    if (!Object.is(Reflect.get(avant, cle), Reflect.get(apres, cle))) return false;
  }
  return true;
};

export const Boite = memo(BoiteVisible, memesProprietes);
Boite.displayName = "Boite";
