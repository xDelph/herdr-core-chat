/**
 * Le mur live (`#/mur`, maquette « Tablet · live wall », 2026-11-10) : une
 * tablette posée chez soi, tout d'un coup d'œil, sans rien toucher.
 *
 * ```
 * 14:32   ● Live           [2 need you] [3 working] [2 to read]   Exit
 * ┌ à toi (répondable) ┐ ┌ au travail ┐ ┌ au travail ┐
 * │ question + choix   │ │ 6:28       │ │ 1:41       │
 * └────────────────────┘ └────────────┘ └────────────┘
 * ○ 11 idle  [logos]                         Screen kept awake
 * ```
 *
 * ⚠️ **Les mêmes données, les mêmes gestes que la liste** : les cartes « à
 * toi » sont `CarteAttention`, avec sa revalidation et sa confirmation des
 * choix à risque — le mur n'invente aucune réponse. Les sections sont celles
 * de `sections.ts`.
 *
 * ⚠️ **L'écran reste allumé** par l'API Wake Lock quand le navigateur
 * l'accepte ; un refus est écrit, jamais tu (la tablette s'éteindrait sans
 * qu'on sache pourquoi).
 */
import { useEffect, useMemo, useState } from "react";
import type { ConversationSummary } from "@herdr-core-chat/shared";
import { useEnLigne, useMaintenant } from "../app/hooks.ts";
import type { EtatDonnees } from "../donnees.ts";
import { DirectSeul } from "../inbox/Boite.tsx";
import { CarteAttention } from "../inbox/CarteAttention.tsx";
import type { Lectures } from "../lectures.ts";
import { lignesAffichees } from "../lignes-provisoires.ts";
import { apercuTexte, chrono, dossierDe, premierNonVide, quand, titrePropre } from "../libelles.ts";
import { routeToHash } from "../routing.ts";
import { sectionConversations } from "../sections.ts";
import { attenteDeTaches } from "../taches.ts";
import { Marque } from "../ui/atomes.tsx";
import { Icone } from "../ui/Icone.tsx";

/** L'état du verrou d'écran : dit tel quel au pied du mur. */
type Eveil = "attente" | "actif" | "refuse" | "absent";

const LIBELLE_EVEIL: Record<Exclude<Eveil, "attente">, string> = {
  absent: "This browser can't keep the screen awake",
  actif: "Screen kept awake",
  refuse: "The browser refused to keep the screen awake",
};

/**
 * Les agents au repos, par famille (claude, codex…) : un logo et un compte
 * chacun, la famille la plus nombreuse d'abord.
 *
 * ⚠️ Retour du 2026-11-11 : un logo par agent faisait sortir la ligne du mur
 * dès une douzaine d'agents au repos. Par famille, la ligne a au plus autant
 * d'entrées qu'il y a de familles d'agents, quel que soit leur nombre.
 */
export const famillesAuRepos = (
  repos: ReadonlyArray<Pick<ConversationSummary, "agent">>,
): ReadonlyArray<{ readonly kind: string | null; readonly n: number }> => {
  const parFamille = new Map<string | null, number>();
  for (const conversation of repos) {
    parFamille.set(conversation.agent, (parFamille.get(conversation.agent) ?? 0) + 1);
  }
  return [...parFamille]
    .map(([kind, n]) => ({ kind, n }))
    .toSorted((a, b) => b.n - a.n || (a.kind ?? "").localeCompare(b.kind ?? ""));
};

/**
 * Demande que l'écran reste allumé tant que le mur est affiché — et la
 * redemande au retour (le navigateur relâche le verrou quand la page se
 * cache).
 */
const useEcranEveille = (): Eveil => {
  const [eveil, setEveil] = useState<Eveil>("attente");
  useEffect(() => {
    const api = (navigator as Navigator & { wakeLock?: WakeLock }).wakeLock;
    if (api === undefined) {
      setEveil("absent");
      return undefined;
    }
    let verrou: WakeLockSentinel | null = null;
    let fini = false;
    const demande = async () => {
      try {
        const obtenu = await api.request("screen");
        if (fini) {
          await obtenu.release();
          return;
        }
        verrou = obtenu;
        setEveil("actif");
        obtenu.addEventListener("release", () => {
          if (!fini && document.visibilityState === "visible") setEveil("refuse");
        });
      } catch {
        if (!fini) setEveil("refuse");
      }
    };
    void demande();
    const surVisibilite = () => {
      if (document.visibilityState === "visible") void demande();
    };
    document.addEventListener("visibilitychange", surVisibilite);
    return () => {
      fini = true;
      document.removeEventListener("visibilitychange", surVisibilite);
      if (verrou !== null) void relache(verrou);
    };
  }, []);
  return eveil;
};

/** Relâche le verrou en quittant le mur ; déjà relâché par le navigateur : rien à dire. */
const relache = async (verrou: WakeLockSentinel): Promise<void> => {
  try {
    await verrou.release();
  } catch {
    // Le navigateur l'a déjà repris (page cachée) : l'écran n'est plus tenu, c'est le but.
  }
};

/** L'heure du mur, HH:MM, à la minute. */
const heure = (instant: number): string =>
  new Date(instant).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

const TuileTravail = ({
  conversation,
  projet,
  maintenant,
  debutObserve,
}: {
  readonly conversation: ConversationSummary;
  readonly projet: string;
  readonly maintenant: number;
  readonly debutObserve: number | undefined;
}) => {
  const auTravail = conversation.status === "working";
  const debut = conversation.tourDebut ?? debutObserve;
  const titre = titrePropre(conversation.title);
  const activite =
    (auTravail ? conversation.activity : undefined) ??
    attenteDeTaches(conversation.tachesDeFond) ??
    apercuTexte(conversation.lastPrompt ?? "");
  return (
    <a className="tuile" href={routeToHash({ kind: "agent", paneId: conversation.paneId })}>
      <span className="tuile__tete">
        <Marque kind={conversation.agent} />
        <span className="tuile__projet">{projet}</span>
      </span>
      <span className="tuile__titre">{titre.length > 0 ? titre : projet}</span>
      <span className="tuile__chrono num">
        {auTravail && debut !== undefined ? chrono(maintenant - debut) : "Working"}
      </span>
      {activite.length > 0 ? <span className="tuile__activite">{activite}</span> : null}
      <span className="tuile__barre" aria-hidden="true">
        <span />
      </span>
    </a>
  );
};

export const Mur = ({
  donnees,
  lectures,
  onQuitter,
}: {
  readonly donnees: EtatDonnees;
  readonly lectures: Lectures;
  readonly onQuitter: () => void;
}) => {
  const maintenant = useMaintenant(1_000);
  const enLigne = useEnLigne();
  const eveil = useEcranEveille();
  const { overview, debutsTravail } = donnees;

  const conversations = useMemo(
    () => lignesAffichees(donnees.conversations, overview).lignes ?? [],
    [donnees.conversations, overview],
  );
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
  const nomDuProjet = (workspaceId: string) => projets.get(workspaceId) ?? workspaceId;

  const groupes = useMemo(
    () => sectionConversations(conversations, "recent", lectures),
    [conversations, lectures],
  );
  const de = (cle: string) => groupes.find((groupe) => groupe.cle === cle)?.conversations ?? [];
  const attente = de("a-traiter");
  // ⚠️ Au mur, une tuile ne change pas de place à chaque message de l'agent :
  // l'ordre du travail est celui des panes, stable, pas le plus récent.
  const travail = de("en-cours").toSorted((a, b) => a.paneId.localeCompare(b.paneId));
  const nonLus = de("done");
  const repos = de("idle");
  const demiMinute = Math.floor(maintenant / 30_000) * 30_000;

  return (
    <section className="mur" aria-label="Live wall">
      <header className="mur__haut">
        <div className="mur__heure">
          <span className="num">{heure(maintenant)}</span>
          <DirectSeul donnees={donnees} maintenant={maintenant} enLigne={enLigne} />
        </div>
        <div className="mur__comptes">
          {attente.length > 0 ? (
            <span className="etat etat--attn">
              <span className="losange" aria-hidden="true" />
              {attente.length} need{attente.length === 1 ? "s" : ""} you
            </span>
          ) : null}
          {travail.length > 0 ? (
            <span className="etat etat--work">
              <span className="spin" aria-hidden="true" />
              {travail.length} working
            </span>
          ) : null}
          {nonLus.length > 0 ? (
            <span className="etat etat--idle">
              <span className="point mur__point-lu" aria-hidden="true" />
              {nonLus.length} to read
            </span>
          ) : null}
        </div>
        <button type="button" className="bouton mur__sortie" onClick={onQuitter}>
          <Icone nom="x" petite />
          Exit
        </button>
      </header>

      <div className="mur__grille">
        {attente.map((conversation) => (
          <div key={conversation.paneId} className="tuile tuile--attn">
            <CarteAttention
              conversation={conversation}
              projet={nomDuProjet(conversation.workspaceId)}
              titre={titrePropre(conversation.title)}
              maintenant={demiMinute}
            />
          </div>
        ))}
        {travail.map((conversation) => (
          <TuileTravail
            key={conversation.paneId}
            conversation={conversation}
            projet={nomDuProjet(conversation.workspaceId)}
            maintenant={maintenant}
            debutObserve={debutsTravail.get(conversation.paneId)}
          />
        ))}
        {nonLus.length > 0 ? (
          <div className="tuile tuile--lire">
            <h2 className="tuile__section">
              <span className="point mur__point-lu" aria-hidden="true" />
              Finished · to read
            </h2>
            {nonLus.map((conversation) => {
              const titre = titrePropre(conversation.title);
              const projet = nomDuProjet(conversation.workspaceId);
              return (
                <a
                  key={conversation.paneId}
                  className="tuile__lu"
                  href={routeToHash({ kind: "agent", paneId: conversation.paneId })}
                >
                  <Marque kind={conversation.agent} />
                  <span className="tuile__lu-texte">
                    <b>{titre.length > 0 ? titre : projet}</b>
                    <span>
                      {projet}
                      {conversation.timestamp === undefined
                        ? ""
                        : ` · ${quand(conversation.timestamp, demiMinute)}`}
                    </span>
                  </span>
                </a>
              );
            })}
          </div>
        ) : null}
        {attente.length === 0 && travail.length === 0 && nonLus.length === 0 ? (
          <p className="mur__calme">Nothing needs you. No agent is working.</p>
        ) : null}
      </div>

      <footer className="mur__pied">
        {repos.length > 0 ? (
          <span className="mur__repos">
            <span className="anneau" aria-hidden="true" />
            {repos.length} idle
            <span className="mur__familles">
              {famillesAuRepos(repos).map((famille) => (
                <span key={famille.kind ?? ""} className="mur__famille">
                  <Marque kind={famille.kind} />
                  <span className="num">{famille.n}</span>
                </span>
              ))}
            </span>
          </span>
        ) : (
          <span />
        )}
        {eveil === "attente" ? null : (
          <span className={eveil === "actif" ? "mur__eveil" : "mur__eveil mur__eveil--non"}>
            {LIBELLE_EVEIL[eveil]}
          </span>
        )}
      </footer>
    </section>
  );
};
