/**
 * La boîte d'envoi (lot 4, maquette D) : les envois non résolus de tous les
 * agents — ouverte depuis la pastille de l'en-tête.
 *
 * ⚠️ **Les gestes sont ceux de la bulle, pas d'autres.** Le texte et le renvoi
 * viennent de `explicationDe` (`chat/Envois.tsx`) : « Retry (may arrive
 * twice) » reste dit tel quel, et un envoi non confirmé demande d'abord
 * d'aller voir la conversation. Rien n'est renvoyé sans un appui.
 *
 * ⚠️ **La cible est celle de l'envoi** (capturée à sa création) : réessayer
 * d'ici repart vers SON pane, jamais vers la conversation ouverte.
 */
import { lireBrouillon } from "../brouillons.ts";
import { entreesBoite, LIBELLE_GENRE, resumeBoite, sousTitreBoite } from "../boite-envoi.ts";
import { explicationDe } from "../chat/Envois.tsx";
import type { Envoi, MagasinEnvois } from "../envois.ts";
import { quand } from "../libelles.ts";
import { Feuille } from "../ui/Feuille.tsx";

/** Le renvoi proposé par l'explication — « Edit message » n'en est pas un. */
const renvoiDe = (envoi: Envoi): string | null => {
  if (envoi.state !== "uncertain" && envoi.state !== "failed") return null;
  const { action } = explicationDe(envoi, false);
  return action === null || action === "Edit message" ? null : action;
};

export const BoiteEnvoi = ({
  envois,
  magasin,
  titreDe,
  maintenant,
  paneCourant,
  onOuvrir,
  onModifier,
  onClose,
}: {
  readonly envois: ReadonlyArray<Envoi>;
  readonly magasin: Pick<MagasinEnvois, "reessaye" | "annule" | "oublie">;
  /** Le nom affiché d'un pane (titre, projet), ou son identifiant. */
  readonly titreDe: (pane: string) => string;
  readonly maintenant: number;
  /** La conversation ouverte : y remettre un texte écraserait son composeur. */
  readonly paneCourant: string | null;
  readonly onOuvrir: (pane: string) => void;
  /** Remet le texte dans le brouillon du pane, puis ouvre la conversation. */
  readonly onModifier: (pane: string, texte: string) => void;
  readonly onClose: () => void;
}) => {
  const entrees = entreesBoite(envois);
  const resume = resumeBoite(entrees);
  return (
    <Feuille titre="Outbox" onClose={onClose}>
      <p className="boite-envoi__resume">{sousTitreBoite(resume)}</p>
      {entrees.length === 0 ? (
        <p className="discret">Every message you sent has been confirmed or closed.</p>
      ) : (
        <ul className="boite-envoi">
          {entrees.map(({ envoi, genre }) => {
            const explication = explicationDe(envoi, false);
            const renvoi = renvoiDe(envoi);
            // ⚠️ Modifier écrit le brouillon : jamais par-dessus un brouillon
            // existant, ni dans la conversation ouverte (son composeur est
            // déjà monté et ne le relirait pas).
            const modifiable =
              (envoi.state === "failed" || envoi.state === "suppressed") &&
              envoi.pane !== paneCourant &&
              lireBrouillon(envoi.pane).length === 0;
            return (
              <li key={envoi.id} className={`envoi-boite envoi-boite--${genre}`} data-genre={genre}>
                <p className="envoi-boite__tete">
                  <strong>{titreDe(envoi.pane)}</strong>
                  <span className="discret">
                    {" · "}
                    {quand(envoi.creeA, maintenant)}
                    {" · "}
                  </span>
                  <span className="envoi-boite__genre">{LIBELLE_GENRE[genre]}</span>
                </p>
                <p className="envoi-boite__texte">{envoi.text}</p>
                <p className="envoi-boite__etat">{explication.texte}</p>
                {genre === "non-confirme" ? (
                  <p className="discret">
                    Check the conversation before retrying: it may have reached the agent.
                  </p>
                ) : null}
                <div className="envoi-boite__gestes">
                  <button type="button" className="bouton" onClick={() => onOuvrir(envoi.pane)}>
                    {genre === "non-confirme" ? "Open to check" : "Open"}
                  </button>
                  {envoi.state === "queued" ? (
                    <button
                      type="button"
                      className="bouton"
                      onClick={() => {
                        magasin.annule(envoi.id);
                      }}
                    >
                      Cancel
                    </button>
                  ) : null}
                  {renvoi === null ? null : (
                    <button
                      type="button"
                      className="bouton"
                      onClick={() => {
                        magasin.reessaye(envoi.id);
                      }}
                    >
                      {renvoi}
                    </button>
                  )}
                  {modifiable ? (
                    <button
                      type="button"
                      className="bouton"
                      onClick={() => {
                        magasin.oublie(envoi.id);
                        onModifier(envoi.pane, envoi.text);
                      }}
                    >
                      Edit
                    </button>
                  ) : null}
                  {envoi.state === "queued" || envoi.state === "sending" ? null : (
                    <button
                      type="button"
                      className="bouton"
                      onClick={() => magasin.oublie(envoi.id)}
                    >
                      Dismiss
                    </button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
      <p className="discret">Unsent text lives on this device until you close the app.</p>
    </Feuille>
  );
};
