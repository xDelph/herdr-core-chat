/**
 * Le centre de connexion (lot 4, maquette B), ouvert depuis l'indicateur
 * « Live » : chaque maillon de la chaîne, son état et sa cause.
 *
 * ⚠️ **« Retry » relit tout** (la resynchronisation existante, la même que le
 * bouton de la liste) : il ne renvoie rien, ne touche aucun agent.
 */
import { useState } from "react";
import { diagnostic, maillons, pireTon, type Maillon } from "../centre-connexion.ts";
import type { EtatDonnees } from "../donnees.ts";
import { Feuille } from "../ui/Feuille.tsx";

const LIBELLE_TON: Record<Maillon["ton"], string> = {
  attention: "Degraded",
  erreur: "Broken",
  inconnu: "Unknown",
  ok: "OK",
};

export const Connexion = ({
  donnees,
  enLigne,
  derniereVie,
  maintenant,
  onReessayer,
  onClose,
}: {
  readonly donnees: EtatDonnees;
  readonly enLigne: boolean;
  readonly derniereVie: number | null;
  readonly maintenant: number;
  readonly onReessayer: () => void;
  readonly onClose: () => void;
}) => {
  const chaine = maillons({ derniereVie, donnees, enLigne, maintenant });
  const pire = pireTon(chaine);
  const [copie, setCopie] = useState<"copie" | "refusee" | null>(null);
  const copier = async () => {
    try {
      await navigator.clipboard.writeText(diagnostic(chaine));
      setCopie("copie");
    } catch {
      setCopie("refusee");
    }
  };
  return (
    <Feuille titre="Connection" onClose={onClose}>
      <p className={`connexion__bilan connexion__bilan--${pire}`}>
        {pire === "ok"
          ? "Everything answers."
          : pire === "inconnu"
            ? "Some links have not answered yet."
            : "Something in the chain needs attention."}
      </p>
      <ul className="connexion">
        {chaine.map((maillon) => (
          <li
            key={maillon.cle}
            className={`maillon maillon--${maillon.ton}`}
            data-maillon={maillon.cle}
            data-ton={maillon.ton}
          >
            <span className="maillon__point" aria-hidden="true" />
            <div className="maillon__corps">
              <p className="maillon__titre">
                {maillon.titre}
                <span className="sr"> ({LIBELLE_TON[maillon.ton]})</span>
              </p>
              <p className="maillon__texte">{maillon.texte}</p>
              {maillon.detail === null ? null : (
                <p className="maillon__detail mono">{maillon.detail}</p>
              )}
            </div>
          </li>
        ))}
      </ul>
      <div className="connexion__gestes">
        <button
          type="button"
          className="bouton"
          disabled={donnees.chargement}
          onClick={onReessayer}
        >
          {donnees.chargement ? "Checking…" : "Retry"}
        </button>
        <button type="button" className="bouton" onClick={() => void copier()}>
          {copie === "copie" ? "Copied" : "Copy diagnostics"}
        </button>
      </div>
      {copie === "refusee" ? <p className="discret">This browser refused the clipboard.</p> : null}
      <p className="discret">Diagnostics never include message content.</p>
    </Feuille>
  );
};
