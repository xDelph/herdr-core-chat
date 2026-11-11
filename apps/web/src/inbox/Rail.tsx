/**
 * Le rail du bureau (maquette « Desktop », 2026-11-10) : à partir de 1200 px,
 * une colonne à gauche de la liste — les états et les projets comme filtres,
 * lancer, la boîte d'envoi, le mur, les réglages.
 *
 * ⚠️ **Un filtre, pas un autre écran.** Choisir « Working » filtre la même
 * liste (`Boite`, `filtreSection`) ; choisir un projet ouvre son détail
 * (`#/projet/<id>`), comme depuis la liste. Les comptes viennent des mêmes
 * sections que la liste (`sections.ts`) : le rail et la liste ne peuvent pas
 * se contredire.
 */
import { memo, useMemo } from "react";
import { useEnLigne, useMaintenant, useTousEnvois } from "../app/hooks.ts";
import { entreesBoite, resumeBoite } from "../boite-envoi.ts";
import type { EtatDonnees } from "../donnees.ts";
import type { Lectures } from "../lectures.ts";
import { lignesAffichees } from "../lignes-provisoires.ts";
import { dossierDe, premierNonVide } from "../libelles.ts";
import { routeToHash } from "../routing.ts";
import { SECTION_LABEL, sectionConversations, type Section } from "../sections.ts";
import { Icone } from "../ui/Icone.tsx";
import { DirectSeul } from "./Boite.tsx";

/** La forme de chaque état, la même que dans la liste. */
const FORME: Record<Section, string> = {
  "a-traiter": "losange",
  done: "point",
  "en-cours": "spin",
  idle: "anneau",
};

const ORDRE_RAIL: ReadonlyArray<Section> = ["a-traiter", "en-cours", "done", "idle"];

export const Rail = memo(
  ({
    donnees,
    lectures,
    filtre,
    projetCourant,
    onFiltre,
    onLancer,
    onBoiteEnvoi,
    onReglages,
    onConnexion,
  }: {
    readonly donnees: EtatDonnees;
    readonly lectures: Lectures;
    /** L'état choisi, ou `null` : toute la liste. */
    readonly filtre: Section | null;
    /** `#/projet/<id>` ouvert, ou `null`. */
    readonly projetCourant: string | null;
    readonly onFiltre: (section: Section | null) => void;
    readonly onLancer: () => void;
    readonly onBoiteEnvoi: () => void;
    readonly onReglages: () => void;
    readonly onConnexion: () => void;
  }) => {
    const maintenant = useMaintenant(30_000);
    const enLigne = useEnLigne();
    const envoisEnAttente = resumeBoite(entreesBoite(useTousEnvois()));
    const conversations = useMemo(
      () => lignesAffichees(donnees.conversations, donnees.overview).lignes ?? [],
      [donnees.conversations, donnees.overview],
    );
    const comptes = useMemo(() => {
      const parSection = new Map<string, number>();
      for (const groupe of sectionConversations(conversations, "recent", lectures)) {
        parSection.set(groupe.cle, groupe.conversations.length);
      }
      return parSection;
    }, [conversations, lectures]);
    const projets = useMemo(() => {
      const parProjet = new Map<string, number>();
      for (const conversation of conversations) {
        parProjet.set(conversation.workspaceId, (parProjet.get(conversation.workspaceId) ?? 0) + 1);
      }
      return (donnees.overview?.workspaces ?? [])
        .map((espace) => ({
          id: espace.workspaceId,
          n: parProjet.get(espace.workspaceId) ?? 0,
          nom: premierNonVide(espace.label, dossierDe(espace.checkoutPath), espace.workspaceId),
        }))
        .filter((projet) => projet.n > 0)
        .toSorted((a, b) => a.nom.localeCompare(b.nom, "en", { sensitivity: "base" }));
    }, [conversations, donnees.overview]);

    const tout = projetCourant === null && filtre === null;
    return (
      <nav className="rail" aria-label="Filters">
        <div className="rail__marque">
          <span className="rail__logo" aria-hidden="true">
            H
          </span>
          <span className="rail__nom">
            <b>Herdr Chat</b>
            <button
              type="button"
              className="direct__bouton"
              aria-label="Connection details"
              onClick={onConnexion}
            >
              <DirectSeul donnees={donnees} maintenant={maintenant} enLigne={enLigne} />
            </button>
          </span>
        </div>
        <button type="button" className="bouton bouton--principal rail__lancer" onClick={onLancer}>
          <Icone nom="plus" petite />
          New agent
        </button>

        <a
          className={tout ? "rail__item rail__item--on" : "rail__item"}
          href={routeToHash({ kind: "liste" })}
          aria-current={tout ? "page" : undefined}
          onClick={() => onFiltre(null)}
        >
          <Icone nom="term" petite />
          All agents
          <span className="rail__n num">{conversations.length}</span>
        </a>
        {ORDRE_RAIL.map((section) => {
          const n = comptes.get(section) ?? 0;
          const actif = projetCourant === null && filtre === section;
          return (
            <a
              key={section}
              className={`rail__item rail__item--${section}${actif ? " rail__item--on" : ""}`}
              href={routeToHash({ kind: "liste" })}
              aria-current={actif ? "page" : undefined}
              onClick={() => onFiltre(section)}
            >
              <span className={`rail__forme ${FORME[section]}`} aria-hidden="true" />
              {SECTION_LABEL[section]}
              {/* Orange seulement s'il y a vraiment quelqu'un à qui répondre. */}
              <span
                className={
                  section === "a-traiter" && n > 0 ? "rail__n rail__n--attn num" : "rail__n num"
                }
              >
                {n}
              </span>
            </a>
          );
        })}

        {projets.length === 0 ? null : <h2 className="rail__titre">Projects</h2>}
        {projets.map((projet) => {
          const actif = projetCourant === projet.id;
          return (
            <a
              key={projet.id}
              className={actif ? "rail__item rail__item--on" : "rail__item"}
              href={routeToHash({ kind: "projet", workspaceId: projet.id })}
              aria-current={actif ? "page" : undefined}
              onClick={() => onFiltre(null)}
            >
              <span className="rail__projet">{projet.nom}</span>
              <span className="rail__n num">{projet.n}</span>
            </a>
          );
        })}

        <span className="rail__vide" />
        {envoisEnAttente.total === 0 ? null : (
          <button type="button" className="rail__item" onClick={onBoiteEnvoi}>
            <Icone nom="send" petite />
            Outbox
            <span
              className={
                envoisEnAttente.aRegarder > 0 ? "rail__n rail__n--attn num" : "rail__n num"
              }
            >
              {envoisEnAttente.total}
            </span>
          </button>
        )}
        <a className="rail__item" href={routeToHash({ kind: "mur" })}>
          <Icone nom="tuiles" petite />
          Live wall
        </a>
        <button type="button" className="rail__item" onClick={onReglages}>
          <Icone nom="sliders" petite />
          Settings
        </button>
      </nav>
    );
  },
);
Rail.displayName = "Rail";
