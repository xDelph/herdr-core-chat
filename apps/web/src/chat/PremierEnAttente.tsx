/**
 * Le premier message d'un agent lancé d'ici, tant qu'il attend que l'agent
 * soit prêt (`premier-message.ts`). Visible, annulable : « Cancel » le rend au
 * composeur, rien n'est perdu.
 */
import { usePremier } from "../app/hooks.ts";
import { premiers } from "../app/services.ts";

export const PremierEnAttente = ({
  paneId,
  agentNom,
  onRemettre,
}: {
  readonly paneId: string;
  readonly agentNom: string;
  readonly onRemettre: (texte: string) => void;
}) => {
  const premier = usePremier(paneId);
  if (premier === undefined) return null;
  return (
    <div className="envois">
      <div className="moi moi--en-file" data-etat="premier">
        <p className="moi__texte">{premier.texte}</p>
        <output className="moi__etat">
          <span className="moi__file" aria-hidden="true" />
          First message — sent as soon as {agentNom} is ready
          <button
            type="button"
            onClick={() => {
              const texte = premiers.annule(paneId);
              if (texte !== null) onRemettre(texte);
            }}
          >
            Cancel
          </button>
        </output>
      </div>
    </div>
  );
};
