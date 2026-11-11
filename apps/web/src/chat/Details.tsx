/**
 * Le panneau de détails du bureau (maquette « Desktop », 2026-11-10), à
 * droite de la conversation : ce que l'agent est, ce qu'il a touché, ce qui
 * tourne à côté — ce que le téléphone range derrière le menu « ⋯ ».
 *
 * ⚠️ **Rien de neuf, rien de deviné** : les mêmes sources que l'en-tête, la
 * feuille des fichiers et celle des tâches. Il n'apparaît qu'à partir de
 * 1400 px ; plus étroit, il n'est même pas calculé.
 */
import type { AgentView, BackgroundTaskCount } from "@herdr-core-chat/shared";
import { useMedia } from "../app/hooks.ts";
import { touchedFilesOf, type RenderedMessage } from "../blocks.ts";
import { libelleReflexion } from "../libelles.ts";
import { Marque } from "../ui/atomes.tsx";
import { Icone } from "../ui/Icone.tsx";

/** La largeur à partir de laquelle le panneau s'ouvre (`conversation.css`). */
export const LARGEUR_DETAILS = "(min-width: 1400px)";

/** Combien de fichiers le panneau liste avant « Show all ». */
const FICHIERS_VISIBLES = 6;

const derniers = (chemin: string): string => chemin.split("/").slice(-3).join("/");

export const Details = ({
  kind,
  agentNom,
  modele,
  reflexion,
  branche,
  cwd,
  agent,
  messages,
  sousAgents,
  tachesDeFond,
  inactif,
  onModele,
  onFichiers,
  onTaches,
  onOuvrirFichier,
}: {
  readonly kind: string | null;
  readonly agentNom: string;
  readonly modele: string | undefined;
  readonly reflexion: string | undefined;
  readonly branche: string | undefined;
  readonly cwd: string | null;
  readonly agent: AgentView | null;
  readonly messages: ReadonlyArray<RenderedMessage>;
  readonly sousAgents: number;
  readonly tachesDeFond: BackgroundTaskCount | undefined;
  readonly inactif: boolean;
  readonly onModele: () => void;
  readonly onFichiers: () => void;
  readonly onTaches: () => void;
  readonly onOuvrirFichier: (chemin: string) => void;
}) => {
  const visible = useMedia(LARGEUR_DETAILS);
  if (!visible) return null;
  const fichiers = touchedFilesOf(messages);
  const ajoutees = fichiers.reduce((somme, fichier) => somme + fichier.added, 0);
  const retirees = fichiers.reduce((somme, fichier) => somme + fichier.removed, 0);
  const reglages = [modele, reflexion === undefined ? undefined : libelleReflexion(reflexion)]
    .filter((morceau): morceau is string => morceau !== undefined && morceau.length > 0)
    .join(" · ");
  const onglet =
    agent === null
      ? null
      : [
          agent.tabLabel ?? agent.workspaceLabel,
          agent.paneRank === null ? null : `pane ${String(agent.paneRank)}`,
        ]
          .filter((morceau): morceau is string => morceau !== null && morceau.length > 0)
          .join(" · ");
  const aCote = sousAgents + (tachesDeFond?.enCours ?? 0);

  return (
    <aside className="details" aria-label="Details">
      <section className="details__bloc">
        <h2 className="details__titre">Agent</h2>
        <div className="details__agent">
          <Marque kind={kind} />
          <div>
            <b>{agentNom}</b>
            {reglages.length > 0 ? <p className="discret">{reglages}</p> : null}
          </div>
        </div>
        <dl className="details__liste">
          {branche === undefined ? null : (
            <>
              <dt>Branch</dt>
              <dd className="mono">{branche}</dd>
            </>
          )}
          {cwd === null ? null : (
            <>
              <dt>Folder</dt>
              <dd className="mono" title={cwd}>
                {derniers(cwd)}
              </dd>
            </>
          )}
          {onglet === null || onglet.length === 0 ? null : (
            <>
              <dt>Herdr tab</dt>
              <dd>{onglet}</dd>
            </>
          )}
        </dl>
        <button type="button" className="bouton" disabled={inactif} onClick={onModele}>
          <Icone nom="sliders" petite />
          Model and thinking…
        </button>
      </section>

      <section className="details__bloc">
        <h2 className="details__titre">
          Changes
          {fichiers.length === 0 ? null : (
            <span className="details__compte num">
              {fichiers.length} file{fichiers.length > 1 ? "s" : ""}{" "}
              <span className="details__plus">+{ajoutees}</span>{" "}
              <span className="details__moins">−{retirees}</span>
            </span>
          )}
        </h2>
        {fichiers.length === 0 ? (
          <p className="discret">No writes in the loaded part of the session.</p>
        ) : (
          <>
            <ul className="details__fichiers">
              {fichiers.slice(0, FICHIERS_VISIBLES).map((fichier) => (
                <li key={fichier.path}>
                  <button
                    type="button"
                    className="details__fichier"
                    title={fichier.path}
                    onClick={() => onOuvrirFichier(fichier.path)}
                  >
                    <span className="mono">{derniers(fichier.path)}</span>
                    <span className="details__plus num">+{fichier.added}</span>
                    <span className="details__moins num">−{fichier.removed}</span>
                  </button>
                </li>
              ))}
            </ul>
            <button type="button" className="lien" onClick={onFichiers}>
              {fichiers.length > FICHIERS_VISIBLES
                ? `Show all ${String(fichiers.length)} files`
                : "Open the diffs"}
            </button>
          </>
        )}
      </section>

      <section className="details__bloc">
        <h2 className="details__titre">Beside the agent</h2>
        {aCote === 0 && (tachesDeFond?.total ?? 0) === 0 ? (
          <p className="discret">No subagent or background task.</p>
        ) : (
          <button type="button" className="lien" onClick={onTaches}>
            {[
              sousAgents > 0 ? `${String(sousAgents)} subagent${sousAgents > 1 ? "s" : ""}` : null,
              tachesDeFond === undefined || tachesDeFond.total === 0
                ? null
                : `${String(tachesDeFond.enCours)} of ${String(tachesDeFond.total)} background tasks running`,
            ]
              .filter((morceau) => morceau !== null)
              .join(" · ")}
          </button>
        )}
      </section>
    </aside>
  );
};
