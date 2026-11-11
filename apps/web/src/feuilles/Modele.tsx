/**
 * Changer le modèle et la réflexion d'un agent **en session** — par le démon
 * (H24, `POST /model`), qui lit le type de l'agent chez Herdr, tape SA
 * recette et relit l'écran (`interactions::reglage`) :
 *
 * - Pi : `/model <fournisseur/id>`, `/thinking <niveau>` ;
 * - Codex : `/model <id> --reasoning=<niveau>` ;
 * - devin : `/model <nom>` (pas de commande de réflexion) ;
 * - Claude Code : son sélecteur `/model`, effort aux flèches, touche `s` —
 *   « cette session seulement ». ⚠️ Tapées, `/model` et `/effort` de Claude
 *   ENREGISTRENT le choix comme défaut global (`~/.claude/settings.json`).
 *
 * « Applied » n'est dit que sur la confirmation affichée par l'agent.
 * ⚠️ Recettes relevées dans le code livré des agents, **non vérifiées par
 * exécution** (2026-11-08) : un écran inattendu arrête tout et le dit.
 *
 * Rien ne part avant « Apply ».
 *
 * ⚠️ **Changer la seule réflexion ne dépend pas du catalogue.** Un modèle
 * courant absent du catalogue (alias, modèle local) bloquait aussi la
 * réflexion ; les niveaux viennent alors de ceux que le catalogue déclare pour
 * le type.
 */
import { useEffect, useState } from "react";
import type { ModelCatalogResponse } from "@herdr-core-chat/shared";
import { changeModel, fetchModelCatalog, reportError, type ModelChangeResult } from "../api.ts";
import { libelleReflexion } from "../libelles.ts";
import { Avis } from "../ui/atomes.tsx";
import { Feuille } from "../ui/Feuille.tsx";

/** Le texte d'un réglage tranché, tel que l'écran de l'agent l'a montré. */
export const decrisReglage = (resultat: ModelChangeResult): string => {
  if (resultat.state === "appliquee") return `Applied — ${resultat.detail}`;
  if (resultat.state === "transmise") return `Sent, not confirmed — ${resultat.detail}`;
  return `Not changed — ${resultat.detail}`;
};

const fournisseurDe = (id: string): string | null => {
  const separateur = id.indexOf("/");
  return separateur < 0 ? null : id.slice(0, separateur);
};

/**
 * Les niveaux de réflexion proposables : ceux du modèle choisi ; sans modèle
 * reconnu, ceux que le catalogue déclare pour le type (sinon l'union de ceux
 * de ses modèles). Rien d'inventé.
 */
export const niveauxProposes = (
  catalogue: ModelCatalogResponse | null,
  modele: { readonly thinkingLevels: ReadonlyArray<string> } | null,
): ReadonlyArray<string> => {
  if (modele !== null) return modele.thinkingLevels;
  if (catalogue === null) return [];
  if (catalogue.thinkingLevels.length > 0) return catalogue.thinkingLevels;
  return [...new Set(catalogue.models.flatMap((candidat) => candidat.thinkingLevels))];
};

/**
 * Les types dont le démon sait changer le MODÈLE en session — miroir de
 * `KINDS_MODELE_EN_SESSION` (`interactions::reglage`). Le démon refuse de
 * toute façon ce qu'il ne sait pas faire : ceci n'évite que d'offrir l'inutile.
 */
export const modeleEnSession = (kind: string): boolean =>
  kind === "pi" || kind === "codex" || kind === "claude" || kind === "devin";

/** Les types dont le démon sait changer la RÉFLEXION en session. */
export const reflexionEnSession = (kind: string): boolean =>
  kind === "pi" || kind === "codex" || kind === "claude";

/** Ce que chaque agent garde du changement — dit avant « Apply ». */
const portee = (kind: string): string => {
  if (kind === "claude")
    return "Claude applies it to this session only: your default is unchanged.";
  if (kind === "pi") return "Pi applies it to this session only.";
  return "Applied to the running session.";
};

/** Le modèle courant tel que le catalogue l'écrit — `null` s'il n'y figure pas. */
const modeleDuCatalogue = (
  catalogue: ModelCatalogResponse,
  kind: string,
  modele: string | undefined,
  fournisseur: string | undefined,
): string | null => {
  if (modele === undefined) return null;
  if (catalogue.models.some((candidat) => candidat.id === modele)) return modele;
  if (kind !== "pi") return null;
  if (fournisseur !== undefined) {
    const qualifie = `${fournisseur}/${modele}`;
    if (catalogue.models.some((candidat) => candidat.id === qualifie)) return qualifie;
  }
  const suffixes = catalogue.models.filter((candidat) => candidat.id.endsWith(`/${modele}`));
  return suffixes.length === 1 ? (suffixes[0]?.id ?? null) : null;
};

export const Modele = ({
  paneId,
  kind,
  agentNom,
  modeleActuel,
  fournisseurActuel,
  reflexionActuelle,
  statut,
  onClose,
}: {
  /**
   * Le statut observé de l'agent. ⚠️ Le réglage n'est offert qu'au REPOS
   * (`idle`, `done`) : la recette tape `/model` puis Entrée, qui validerait
   * un choix sur une question, et partirait dans la file d'un agent au
   * travail (B1.3 — le serveur refuse aussi).
   */
  readonly statut: string | undefined;
  readonly paneId: string;
  readonly kind: string;
  readonly agentNom: string;
  readonly modeleActuel: string | undefined;
  readonly fournisseurActuel: string | undefined;
  readonly reflexionActuelle: string | undefined;
  readonly onClose: () => void;
}) => {
  const [catalogue, setCatalogue] = useState<ModelCatalogResponse | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [fournisseur, setFournisseur] = useState<string | null | undefined>(undefined);
  const [choix, setChoix] = useState<string | null | undefined>(undefined);
  const [reflexion, setReflexion] = useState<string | null | undefined>(undefined);
  const [filtre, setFiltre] = useState("");
  const [resultat, setResultat] = useState<ModelChangeResult | null>(null);
  const [occupe, setOccupe] = useState(false);
  const auRepos = statut === "idle" || statut === "done";

  useEffect(() => {
    let vivant = true;
    void (async () => {
      try {
        const lu = await fetchModelCatalog(kind);
        if (vivant) setCatalogue(lu);
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        if (vivant) setErreur(message);
        reportError("failed to read the model catalog", { kind, message });
      }
    })();
    return () => {
      vivant = false;
    };
  }, [kind]);

  const actuel =
    catalogue === null ? null : modeleDuCatalogue(catalogue, kind, modeleActuel, fournisseurActuel);
  const modeleId = choix === undefined ? actuel : choix;
  const modele = catalogue?.models.find((candidat) => candidat.id === modeleId) ?? null;
  const fournisseurCourant =
    fournisseur === undefined
      ? (fournisseurActuel ?? (actuel === null ? null : fournisseurDe(actuel)))
      : fournisseur;
  const fournisseurs =
    kind === "pi" && catalogue !== null
      ? [
          ...new Set(
            catalogue.models
              .map((candidat) => fournisseurDe(candidat.id))
              .filter((f) => f !== null),
          ),
        ]
      : [];
  const visibles =
    catalogue?.models.filter(
      (candidat) =>
        (kind !== "pi" ||
          fournisseurCourant === null ||
          fournisseurDe(candidat.id) === fournisseurCourant) &&
        `${candidat.id} ${candidat.label ?? ""}`
          .toLowerCase()
          .includes(filtre.trim().toLowerCase()),
    ) ?? [];
  const niveau = reflexion === undefined ? (reflexionActuelle ?? null) : reflexion;
  const enSession = reflexionEnSession(kind);
  const niveaux = niveauxProposes(catalogue, modele);
  const changeModele = modele !== null && modele.id !== actuel;
  const changeReflexion =
    enSession &&
    niveau !== null &&
    niveau !== (reflexionActuelle ?? null) &&
    niveaux.includes(niveau);

  // ⚠️ Claude règle l'effort DANS son sélecteur de modèle, sur le modèle
  // surligné : changer le seul effort exige de nommer le modèle courant (le
  // démon en tire l'alias : `claude-opus-…` → `opus`).
  const modeleEnvoye = changeModele
    ? modele.id
    : kind === "claude" && changeReflexion
      ? (actuel ?? modeleActuel)
      : undefined;
  const manqueModele = kind === "claude" && changeReflexion && modeleEnvoye === undefined;

  const appliquer = async () => {
    if (occupe || !auRepos || manqueModele || (!changeModele && !changeReflexion)) return;
    setOccupe(true);
    try {
      setResultat(
        await changeModel(paneId, {
          ...(modeleEnvoye === undefined ? {} : { model: modeleEnvoye }),
          ...(changeReflexion && niveau !== null ? { thinking: niveau } : {}),
        }),
      );
    } finally {
      setOccupe(false);
    }
  };

  if (!modeleEnSession(kind)) {
    return (
      <Feuille titre={`Model · ${agentNom}`} onClose={onClose}>
        <Avis ton="attention">
          {kind === "opencode"
            ? "opencode's model and variant can't be changed from here: its pickers mark the selected row by colour only, so the choice could not be checked before pressing Enter. Use ctrl+x m and ctrl+t on the Mac."
            : `${agentNom}'s model can't be changed mid-session from here: no in-session command is known for ${kind}.`}
        </Avis>
      </Feuille>
    );
  }

  return (
    <Feuille titre={`Model · ${agentNom}`} onClose={onClose}>
      {resultat !== null ? (
        <output className="feuille__resultats">
          <p>{decrisReglage(resultat)}</p>
        </output>
      ) : null}
      {erreur !== null ? (
        <Avis ton="erreur">
          Unreadable {kind} catalog: {erreur}
        </Avis>
      ) : catalogue === null ? (
        <output className="discret">Loading the catalog…</output>
      ) : catalogue.models.length === 0 ? (
        <Avis ton="attention">{catalogue.error ?? `No verified catalog for ${kind}.`}</Avis>
      ) : (
        <>
          {kind === "pi" ? (
            <fieldset className="champ">
              <legend>Provider</legend>
              <select
                className="choix-liste"
                value={fournisseurCourant ?? ""}
                disabled={occupe}
                onChange={(evenement) => {
                  setFournisseur(evenement.target.value || null);
                  setChoix(null);
                  setReflexion(null);
                  setFiltre("");
                }}
              >
                <option value="">Choose…</option>
                {fournisseurs.map((candidat) => (
                  <option key={candidat} value={candidat}>
                    {candidat}
                  </option>
                ))}
              </select>
            </fieldset>
          ) : null}
          <fieldset className="champ">
            <legend>Model</legend>
            {visibles.length > 8 || filtre.length > 0 ? (
              <input
                type="search"
                className="champ__filtre"
                value={filtre}
                placeholder={`Filter ${String(visibles.length)} models`}
                aria-label="Filter models"
                onChange={(evenement) => setFiltre(evenement.target.value)}
              />
            ) : null}
            <div className="modeles">
              {visibles.slice(0, 200).map((candidat) => (
                <label key={candidat.id} className="modele">
                  <input
                    type="radio"
                    name="modele"
                    checked={candidat.id === modeleId}
                    disabled={occupe}
                    onChange={() => {
                      setChoix(candidat.id);
                      setReflexion((precedente) => {
                        const garde = precedente === undefined ? reflexionActuelle : precedente;
                        return garde !== undefined &&
                          garde !== null &&
                          candidat.thinkingLevels.includes(garde)
                          ? garde
                          : null;
                      });
                    }}
                  />
                  <span>{candidat.label ?? candidat.id}</span>
                  <span className="modele__id">
                    {candidat.id === actuel ? "current" : candidat.id}
                  </span>
                </label>
              ))}
              {visibles.length === 0 ? <p className="discret modele">No model matches.</p> : null}
            </div>
            <p className="discret">
              {actuel === null
                ? "The current model is not declared in the session."
                : `Current: ${actuel}`}
            </p>
          </fieldset>
        </>
      )}
      {!enSession ? (
        <p className="discret">
          {agentNom}'s thinking level cannot be set mid-session from here: {kind} has no command for
          a given level.
          {catalogue !== null && catalogue.thinkingLevels.length > 0
            ? ` Levels offered at launch: ${catalogue.thinkingLevels.map(libelleReflexion).join(", ")}.`
            : ""}
        </p>
      ) : niveaux.length > 0 ? (
        <fieldset className="champ">
          <legend>Thinking</legend>
          <fieldset className="seg seg--plein" aria-label="Thinking level">
            {niveaux.map((candidat) => (
              <button
                key={candidat}
                type="button"
                aria-pressed={niveau === candidat}
                disabled={occupe}
                onClick={() => setReflexion(candidat)}
              >
                {libelleReflexion(candidat)}
              </button>
            ))}
          </fieldset>
          <p className="discret">
            {reflexionActuelle === undefined
              ? "Current level unknown."
              : `Current: ${libelleReflexion(reflexionActuelle)}`}
            {modele === null && catalogue !== null
              ? " — levels declared for the agent type, since the current model is not in the catalog."
              : ""}
          </p>
        </fieldset>
      ) : modele !== null ? (
        <p className="discret">This model declares no thinking level.</p>
      ) : catalogue !== null ? (
        <p className="discret">The catalog declares no thinking level.</p>
      ) : null}
      <button
        type="button"
        className="bouton bouton--principal bouton--plein"
        disabled={occupe || !auRepos || manqueModele || (!changeModele && !changeReflexion)}
        onClick={() => void appliquer()}
      >
        {occupe ? "Applying…" : "Apply"}
      </button>
      <p className="discret">
        {!auRepos
          ? statut === "blocked"
            ? "The agent is waiting for an answer: answer it first, then change the model."
            : "The agent is not idle: wait until it finishes its turn to change the model."
          : manqueModele
            ? "The current model is unknown: choose the model too, Claude sets the effort in its model picker."
            : `${portee(kind)} “Applied” is only shown once the agent confirms it on screen.`}
      </p>
    </Feuille>
  );
};
