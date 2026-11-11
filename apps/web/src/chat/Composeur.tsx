/**
 * Le composeur — isolé du fil.
 *
 * ⚠️ **Taper ne redessine pas la conversation.** Le brouillon vit ici, et ici
 * seulement : l'écran parent ne reçoit le texte qu'à l'envoi. Avant, chaque
 * frappe repassait par l'écran entier (trente états), donc par le fil.
 *
 * ⚠️ **Arrêter reste visible tant que l'agent travaille** — champ vide ou non,
 * focus ou non. L'ancien composeur cachait le bouton dès qu'on tapait : on ne
 * pouvait plus arrêter un agent pendant qu'on lui écrivait la correction.
 *
 * ⚠️ **Un agent bloqué, en erreur ou muet s'arrête aussi** (B1.14) : champ
 * vide, Stop prend la place d'Envoyer — le démon accepte désormais de
 * l'interrompre.
 *
 * ⚠️ Un brouillon par pane, en mémoire (`brouillons.ts`) : jamais écrit sur le
 * disque, jamais partagé entre deux agents.
 *
 * ⚠️ **Une photo devient un CHEMIN dans le brouillon (lot 4).** Elle est posée
 * à côté du terminal de l'agent (`terminal.put`), puis son chemin s'ajoute au
 * message, sur sa ligne — rien n'est envoyé tant qu'on n'appuie pas sur
 * Envoyer. La ligne d'état du dépôt n'existe que pendant un dépôt ou après un
 * échec : sans pièce jointe, le composeur garde sa hauteur.
 */
import { suitLeBas } from "../defilement.ts";
import {
  memo,
  useCallback,
  useId,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type Ref,
} from "react";
import { ecrireBrouillon, lireBrouillon } from "../brouillons.ts";
import { Icone } from "../ui/Icone.tsx";

export interface PoigneeComposeur {
  readonly remettre: (texte: string) => void;
}

/** Ce que le dépôt d'une photo rend : le chemin à coller dans le message. */
export type Deposeur = (fichier: File) => Promise<{ readonly path: string }>;

/** Le brouillon avec le chemin d'une pièce jointe, sur sa propre ligne. */
export const avecPieceJointe = (brouillon: string, chemin: string): string => {
  const debut = brouillon.trimEnd();
  return debut.length === 0 ? `${chemin}\n` : `${debut}\n${chemin}\n`;
};

type EtatPiece =
  | { readonly phase: "envoi"; readonly nom: string }
  | { readonly phase: "erreur"; readonly nom: string; readonly texte: string };

const HAUTEUR_LIGNE = 21;
const LIGNES_MAX = 6;

export const Composeur = memo(
  ({
    paneId,
    agentNom,
    travaille,
    interruptible = travaille,
    arretEnCours,
    aide,
    envoiImpossible = false,
    inactif = false,
    onEnvoie,
    onArrete,
    deposer,
    ref,
  }: {
    readonly paneId: string;
    readonly agentNom: string;
    readonly travaille: boolean;
    /** Stop a un sens : l'agent travaille, attend, est en erreur ou ne dit rien. */
    readonly interruptible?: boolean;
    readonly arretEnCours: boolean;
    readonly aide: string | null;
    /**
     * L'envoi n'est pas possible maintenant (session en lecture, question à
     * l'écran…) : le brouillon reste modifiable, `aide` dit pourquoi.
     */
    readonly envoiImpossible?: boolean;
    /** Le pane n'existe plus : ni saisie, ni envoi, ni arrêt. */
    readonly inactif?: boolean;
    readonly onEnvoie: (texte: string) => void;
    readonly onArrete: () => void;
    /** Pose une photo à côté du terminal ; absent : pas de bouton de pièce jointe. */
    readonly deposer?: Deposeur | undefined;
    readonly ref?: Ref<PoigneeComposeur>;
  }) => {
    const [brouillon, setBrouillon] = useState(() => lireBrouillon(paneId));
    const champ = useRef<HTMLTextAreaElement | null>(null);
    const selecteur = useRef<HTMLInputElement | null>(null);
    const [piece, setPiece] = useState<EtatPiece | null>(null);
    const id = useId();

    /*
      ⚠️ **Le fil ne saute plus quand le champ grandit** (retour du
      2026-11-11). Pour se mesurer, le champ se replie un instant à `auto` :
      le fil, au-dessus, grandissait d'autant le temps du calcul, le
      navigateur y bornait le défilement, puis le champ reprenait sa hauteur —
      le texte de la conversation descendait d'un cran, puis remontait quand
      l'observateur du fil rattrapait le bas. La position du fil est donc
      relevée AVANT la mesure et rendue APRÈS, dans le même calcul, avant
      toute peinture : au bas, il reste au bas ; plus haut, il ne bouge pas.
    */
    const ajuste = useCallback(() => {
      const element = champ.current;
      if (element === null) return;
      const fil =
        element.closest(".conversation__principal")?.querySelector<HTMLElement>(".fil") ?? null;
      const position =
        fil === null
          ? null
          : {
              auBas: suitLeBas(fil),
              scrollTop: fil.scrollTop,
            };
      const maximum = HAUTEUR_LIGNE * LIGNES_MAX + 24;
      element.style.height = "auto";
      element.style.height = `${String(Math.min(element.scrollHeight, maximum))}px`;
      element.style.overflowY = element.scrollHeight > maximum ? "auto" : "hidden";
      if (fil === null || position === null) return;
      fil.scrollTop = position.auBas ? fil.scrollHeight : position.scrollTop;
    }, []);

    // Avant la peinture : sinon une image passait avec l'ancienne hauteur (le
    // texte saisi glissait dans le champ, puis le champ grandissait).
    useLayoutEffect(ajuste, [ajuste, brouillon]);

    const change = useCallback(
      (texte: string) => {
        setBrouillon(texte);
        ecrireBrouillon(paneId, texte);
      },
      [paneId],
    );

    useImperativeHandle(
      ref,
      () => ({
        remettre: (texte: string) => {
          change(texte);
          champ.current?.focus();
        },
      }),
      [change],
    );

    const joint = useCallback(
      async (fichier: File) => {
        if (deposer === undefined) return;
        setPiece({ nom: fichier.name, phase: "envoi" });
        try {
          const depose = await deposer(fichier);
          // Le brouillon le plus récent : un dépôt qui finit après une frappe
          // ajoute son chemin à CE texte, pas à celui du début du dépôt.
          setBrouillon((courant) => {
            const suivant = avecPieceJointe(courant, depose.path);
            ecrireBrouillon(paneId, suivant);
            return suivant;
          });
          setPiece(null);
          champ.current?.focus();
        } catch (erreur) {
          setPiece({
            nom: fichier.name,
            phase: "erreur",
            texte: erreur instanceof Error ? erreur.message : String(erreur),
          });
        }
      },
      [deposer, paneId],
    );

    const bloque = envoiImpossible || inactif;
    const envoie = useCallback(() => {
      const texte = brouillon.trim();
      if (texte.length === 0 || bloque) return;
      onEnvoie(texte);
      change("");
    }, [bloque, brouillon, change, onEnvoie]);

    const vide = brouillon.trim().length === 0;
    const montrerStop = interruptible && !inactif && (travaille || vide);

    return (
      <form
        className="composeur"
        onSubmit={(evenement) => {
          evenement.preventDefault();
          envoie();
        }}
      >
        {piece === null ? null : (
          <div
            className={
              piece.phase === "erreur"
                ? "composeur__piece composeur__piece--erreur"
                : "composeur__piece"
            }
            role={piece.phase === "erreur" ? "alert" : "status"}
          >
            {piece.phase === "envoi" ? (
              <>
                <span className="spin" aria-hidden="true" />
                <span>Uploading {piece.nom}…</span>
              </>
            ) : (
              <>
                <span>
                  {piece.nom} was not attached: {piece.texte}
                </span>
                <button
                  type="button"
                  className="bouton-icone"
                  aria-label="Dismiss"
                  onClick={() => setPiece(null)}
                >
                  <Icone nom="x" petite />
                </button>
              </>
            )}
          </div>
        )}
        <div className="composeur__champ">
          {deposer === undefined ? null : (
            <>
              <input
                ref={selecteur}
                type="file"
                accept="image/*"
                className="sr"
                tabIndex={-1}
                aria-hidden="true"
                onChange={(evenement) => {
                  const fichier = evenement.target.files?.[0];
                  // Le même fichier choisi deux fois doit redéclencher `change`.
                  evenement.target.value = "";
                  if (fichier !== undefined) void joint(fichier);
                }}
              />
              <button
                type="button"
                className="rond rond--piece"
                aria-label="Attach a photo"
                disabled={inactif || piece?.phase === "envoi"}
                onClick={() => selecteur.current?.click()}
              >
                <Icone nom="clip" />
              </button>
            </>
          )}
          <label className="sr" htmlFor={id}>
            Message {agentNom}
          </label>
          <textarea
            ref={champ}
            id={id}
            rows={1}
            value={brouillon}
            placeholder={inactif ? "Pane closed" : `Message ${agentNom}…`}
            disabled={inactif}
            aria-describedby={aide === null || aide === "" ? undefined : `${id}-aide`}
            // Entrée insère une ligne : la touche du clavier le dit.
            enterKeyHint="enter"
            onChange={(evenement) => change(evenement.target.value)}
            onKeyDown={(evenement) => {
              // Entrée seule fait un retour à la ligne (un téléphone n'a pas
              // d'autre touche pour ça) ; ⌘/Ctrl + Entrée envoie.
              if (evenement.key === "Enter" && (evenement.metaKey || evenement.ctrlKey)) {
                evenement.preventDefault();
                envoie();
              }
            }}
          />
          {montrerStop ? (
            <button
              type="button"
              className="rond rond--stop"
              aria-label="Stop the agent"
              disabled={arretEnCours}
              onClick={onArrete}
            >
              <span className="carre" aria-hidden="true" />
            </button>
          ) : null}
          {!montrerStop || !vide ? (
            <button
              type="submit"
              className="rond rond--envoi"
              aria-label="Send"
              disabled={vide || bloque}
            >
              <Icone nom="up" />
            </button>
          ) : null}
        </div>
        {/*
          ⚠️ Plus d'identifiant de pane ici (`wB:p1D`) : c'est un repère interne
          d'Herdr, il ne dit rien à l'utilisateur (même retour que D015.2).
        */}
        {aide === null || aide === "" ? null : (
          <div className="composeur__aide" id={`${id}-aide`}>
            {aide}
          </div>
        )}
      </form>
    );
  },
);
Composeur.displayName = "Composeur";
