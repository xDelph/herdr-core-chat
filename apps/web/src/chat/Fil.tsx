/**
 * Le fil d'une conversation : défilement, historique, envois, travail en cours.
 *
 * Règles de défilement (`defilement.ts`, testées) :
 *
 * - à l'ouverture, le message retenu pour ce pane pendant cet onglet (oublié
 *   si du neuf est arrivé depuis), sinon le bas — ou la dernière demande si la
 *   réponse n'a pas encore été vue (`done`) ;
 * - on suit le bas tant qu'on y est ; on ne tire jamais quelqu'un de sa
 *   lecture : du neuf pendant qu'on lit plus haut affiche « Nouveaux messages » ;
 * - l'historique se charge en approchant du haut, et la vue ne saute pas
 *   (`positionApresPrepend`) ;
 * - quand l'agent finit pendant qu'on suit le bas **page cachée**, la vue se
 *   pose sur la demande, pour lire la réponse avec sa question ; s'il finit
 *   sous les yeux, on reste au bas — on vient de la lire s'écrire.
 *
 * ⚠️ **Défiler ne touche pas au Mac.** L'ancien fil posait le focus Herdr
 * (`POST /focus`) après un défilement : lire volait le clavier du bureau.
 *
 * ⚠️ **Remonter le fil ne doit pas saccader.** Safari n'ajuste pas la position
 * de défilement quand le contenu AU-DESSUS change de taille (pas de « scroll
 * anchoring »). Deux sources de sauts sont donc fermées :
 *
 * - plus de `content-visibility: auto` sur les éléments : chaque élément qui
 *   entrait à l'écran remplaçait sa hauteur estimée (96 px) par la vraie, et
 *   tout le contenu sautait à chaque message ;
 * - l'historique ne s'insère pas pendant un défilement : la page plus ancienne
 *   est demandée quand le défilement s'arrête, et la correction de position se
 *   mesure à l'insertion (hauteur du dernier rendu, position actuelle) — jamais
 *   contre l'inertie d'iOS avec une mesure périmée.
 *
 * La mémoïsation par élément fait qu'un delta ne redessine que le tour qui bouge.
 *
 * ⚠️ **Le DOM est borné par une fenêtre glissante** (`fenetre-fil.ts`) : seuls
 * les derniers tours sont rendus. Remonter au haut de la fenêtre l'étend
 * d'abord (au repos, position corrigée comme pour l'historique), puis seulement
 * charge une page plus ancienne. Revenu au bas, la fenêtre re-glisse.
 *
 * ⚠️ **L'arrêt du défilement ne mesure plus tout le fil** : un
 * `IntersectionObserver` tient les éléments à l'écran, et seuls eux sont
 * mesurés pour retenir la position (avant : `getBoundingClientRect` sur
 * chaque élément, à chaque arrêt).
 */
import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { AgentStatus } from "@herdr-core-chat/shared";
import type { Item } from "../blocks.ts";
import {
  ancrage,
  positionApresPrepend,
  SEUIL_PREMIERES_LIGNES_PX,
  suitLeBas,
} from "../defilement.ts";
import { debutEtendu, debutParDefaut, debutPourAncre, doitReglisser } from "../fenetre-fil.ts";
import type { SuiviSession, VueSession } from "../flux-session.ts";
import {
  ecritPosition,
  litPosition,
  premierVisible,
  repriseDe,
  stockageSession,
} from "../memoire-defilement.ts";
import { Avis } from "../ui/atomes.tsx";
import { SqueletteFil } from "../ui/Squelettes.tsx";
import { Icone } from "../ui/Icone.tsx";
import { Annonces } from "./Annonces.tsx";
import { ElementFil } from "./Elements.tsx";

const cleDe = (item: Item): string => (item.kind === "tools" ? item.id : `m:${item.message.id}`);

/** L'élément `index` ouvre-t-il un tour (une demande) ? */
const demandeDans =
  (items: ReadonlyArray<Item>) =>
  (index: number): boolean => {
    const item = items[index];
    return item?.kind === "single" && item.message.role === "user";
  };

/** La fenêtre épinglée au tout premier élément connu (l'historique s'y ajoute). */
const DEBUT_DU_FIL = "\u0000debut";

const debutDuJour = (): number => {
  const maintenant = new Date();
  return new Date(maintenant.getFullYear(), maintenant.getMonth(), maintenant.getDate()).getTime();
};

/** Sans défilement depuis ce délai, l'utilisateur ne fait plus défiler. */
const REPOS_DEFILEMENT_MS = 160;

/**
 * Les éléments du fil, dans l'ordre des `items` : chaque `ElementFil` rend un
 * seul `.fil__el` racine. Un compte qui ne tombe pas juste ne rend rien — on ne
 * devine pas quel élément est lequel.
 */
const elementsDuFil = (
  contenu: HTMLElement,
  items: ReadonlyArray<Item>,
): ReadonlyArray<{ readonly cle: string; readonly element: Element }> => {
  const elements = contenu.querySelectorAll(":scope > .fil__el");
  if (elements.length !== items.length) return [];
  return items.flatMap((item, index) => {
    const element = elements[index];
    return element === undefined ? [] : [{ cle: cleDe(item), element }];
  });
};

/** Pose la dernière demande en haut de la vue. */
const versDerniereDemande = (conteneur: HTMLElement): void => {
  const demandes = conteneur.querySelectorAll("[data-demande]");
  const derniere = demandes[demandes.length - 1];
  if (derniere === undefined) {
    conteneur.scrollTop = conteneur.scrollHeight;
    return;
  }
  const ecart = derniere.getBoundingClientRect().top - conteneur.getBoundingClientRect().top;
  conteneur.scrollTop += ecart - 36;
};

export const Fil = memo(
  ({
    paneId,
    items,
    vue,
    suivi,
    status,
    cwd,
    aUneSession,
    envoisEnAttente,
    onOuvrirFichier,
    children,
  }: {
    readonly paneId: string;
    readonly items: ReadonlyArray<Item>;
    readonly vue: VueSession;
    readonly suivi: SuiviSession | null;
    readonly status: AgentStatus;
    readonly cwd: string | null;
    readonly aUneSession: boolean;
    readonly envoisEnAttente: number;
    readonly onOuvrirFichier: (chemin: string) => void;
    /** Envois en attente et ligne de travail — toujours en bas du fil. */
    readonly children: ReactNode;
  }) => {
    const conteneur = useRef<HTMLDivElement | null>(null);
    const contenu = useRef<HTMLDivElement | null>(null);
    const sentinelle = useRef<HTMLSpanElement | null>(null);
    const suitBas = useRef(true);
    const poseFaite = useRef(false);
    const statutVu = useRef<AgentStatus | null>(null);
    // La hauteur du contenu au DERNIER rendu : la base de la correction quand
    // l'historique s'insère en haut.
    const hauteurRendue = useRef(0);
    const dernierDefilement = useRef(0);
    const relance = useRef<number | undefined>(undefined);
    const enregistrement = useRef<number | undefined>(undefined);
    const [nouveaux, setNouveaux] = useState(false);
    const [loinDuBas, setLoinDuBas] = useState(false);
    /*
      La fenêtre : la clé de son premier élément (`null` : pas encore posée, la
      fenêtre par défaut ; `DEBUT_DU_FIL` : tout depuis le premier élément).
      ⚠️ Une CLÉ, pas un index : l'historique s'insère en haut, les index
      bougent, la clé reste.
    */
    const [debutCle, setDebutCle] = useState<string | null>(null);
    // Chaque extension vers le haut : la position se corrige comme pour une
    // page d'historique (effet 5).
    const [extensions, setExtensions] = useState(0);
    // `epinglee` : la clé retenue est encore dans le fil (une session
    // remplacée la perd — la fenêtre par défaut revient, puis se refige).
    const { debut, epinglee } = useMemo(() => {
      if (debutCle === DEBUT_DU_FIL) return { debut: 0, epinglee: true };
      const index = debutCle === null ? -1 : items.findIndex((item) => cleDe(item) === debutCle);
      return index >= 0
        ? { debut: index, epinglee: true }
        : { debut: debutParDefaut(items.length, demandeDans(items)), epinglee: false };
    }, [debutCle, items]);
    const rendus = useMemo(() => (debut === 0 ? items : items.slice(debut)), [debut, items]);
    const epingle = useCallback(
      (index: number) => {
        const item = items[index];
        setDebutCle(index <= 0 || item === undefined ? DEBUT_DU_FIL : cleDe(item));
      },
      [items],
    );
    const jour = debutDuJour();
    const dernier = items.at(-1);
    const itemsVus = useRef(rendus);
    useLayoutEffect(() => {
      itemsVus.current = rendus;
    });
    // Les éléments À L'ÉCRAN, tenus par un observateur (pas mesurés à chaque arrêt).
    const aLEcran = useRef(new Set<Element>());
    const observateurEcran = useRef<IntersectionObserver | null>(null);

    // 1. La première pose, une fois le contenu là.
    useLayoutEffect(() => {
      const element = conteneur.current;
      if (poseFaite.current || element === null || items.length === 0) return;
      poseFaite.current = true;
      statutVu.current = status;
      const reprise = repriseDe(
        litPosition(stockageSession(), paneId),
        dernier === undefined ? null : cleDe(dernier),
      );
      if (reprise.genre === "bas") {
        element.scrollTop = element.scrollHeight;
        suitBas.current = true;
        return;
      }
      if (reprise.genre === "ancre" && contenu.current !== null) {
        // ⚠️ Un message retenu plus haut que la fenêtre : elle s'étend jusqu'à
        // lui, et la pose attend le rendu suivant.
        const indexAncre = items.findIndex((item) => cleDe(item) === reprise.ancre);
        const debutAncre = debutPourAncre(debut, indexAncre);
        if (debutAncre !== debut) {
          poseFaite.current = false;
          epingle(debutAncre);
          return;
        }
        const cible = elementsDuFil(contenu.current, rendus).find(
          (candidat) => candidat.cle === reprise.ancre,
        );
        if (cible !== undefined) {
          element.scrollTop +=
            cible.element.getBoundingClientRect().top -
            element.getBoundingClientRect().top -
            reprise.decalage;
          suitBas.current = suitLeBas(element);
          return;
        }
      }
      if (status === "done") {
        versDerniereDemande(element);
        suitBas.current = suitLeBas(element);
        return;
      }
      element.scrollTop = element.scrollHeight;
      suitBas.current = true;
    }, [debut, dernier, epingle, items, paneId, rendus, status]);

    // 1 bis. La fenêtre se fige une fois posée : sans cela, chaque message
    // ajouté la ferait glisser, et retirerait du contenu au-dessus du lecteur.
    useLayoutEffect(() => {
      if (poseFaite.current && !epinglee && items.length > 0) epingle(debut);
    });

    // 1 ter. Revenu au bas, une fenêtre qui a trop grandi re-glisse (le bas est
    // suivi : le contenu retiré est au-dessus, la vue reste au bas).
    useEffect(() => {
      if (!poseFaite.current || !doitReglisser(debut, items.length, suitBas.current)) return;
      epingle(debutParDefaut(items.length, demandeDans(items)));
    }, [debut, epingle, items]);

    // 2. Suivre le bas quand le contenu OU le conteneur change de taille
    //    (composeur qui grandit, clavier iOS qui monte).
    useEffect(() => {
      const element = conteneur.current;
      const interieur = contenu.current;
      if (element === null || interieur === null) return undefined;
      const observateur = new ResizeObserver(() => {
        if (poseFaite.current && suitBas.current) element.scrollTop = element.scrollHeight;
      });
      observateur.observe(interieur);
      observateur.observe(element);
      return () => observateur.disconnect();
    }, []);

    // 3. Du neuf pendant qu'on lit plus haut : on le signale, on ne bouge pas.
    useEffect(() => {
      if (poseFaite.current && !suitBas.current) setNouveaux(true);
    }, [dernier]);

    // 4. L'agent change d'état pendant qu'on suit le bas.
    useEffect(() => {
      const element = conteneur.current;
      if (!poseFaite.current || element === null) return;
      const avant = statutVu.current;
      statutVu.current = status;
      if (avant === status) return;
      const ou = ancrage({
        apres: status,
        avant,
        pageVisible: document.visibilityState === "visible",
        suivaitLeBas: suitBas.current,
      });
      if (ou === "bas") element.scrollTop = element.scrollHeight;
      if (ou === "demande") {
        versDerniereDemande(element);
        suitBas.current = suitLeBas(element);
      }
    }, [status]);

    // 5. L'historique ajouté en haut ne fait pas sauter la lecture : la vue
    //    garde le même message sous les yeux. `overflow-anchor: none` sur le
    //    fil — le navigateur ne corrige rien de lui-même, nous seuls le faisons,
    //    donc pareil sur Safari et Chrome.
    useLayoutEffect(() => {
      const element = conteneur.current;
      if (element === null || hauteurRendue.current === 0) return;
      element.scrollTop = positionApresPrepend({
        hauteurApres: element.scrollHeight,
        hauteurAvant: hauteurRendue.current,
        scrollAvant: element.scrollTop,
      });
    }, [vue.ajoutsEnHaut, extensions]);
    // Après CHAQUE rendu : la hauteur de référence de la prochaine insertion.
    useLayoutEffect(() => {
      const element = conteneur.current;
      if (element !== null) hauteurRendue.current = element.scrollHeight;
    });

    const chargeAncien = useCallback(() => {
      const element = conteneur.current;
      if (suivi === null || element === null) return;
      if (vue.precedent === null || vue.chargementAncien) return;
      void suivi.chargePlusAncien();
    }, [suivi, vue.chargementAncien, vue.precedent]);

    // Remonter : d'abord ce qui est déjà lu mais pas rendu, puis l'historique.
    const etend = useCallback(() => {
      if (debut <= 0) return;
      epingle(debutEtendu(debut, demandeDans(items)));
      setExtensions((compte) => compte + 1);
    }, [debut, epingle, items]);
    const remonte = useCallback(() => {
      if (debut > 0) {
        etend();
        return;
      }
      chargeAncien();
    }, [chargeAncien, debut, etend]);

    // L'historique se demande au repos : insérer une page pendant l'inertie
    // d'iOS obligeait à corriger la position en plein mouvement — l'à-coup.
    const demandeAncien = useCallback(() => {
      if (relance.current !== undefined) window.clearTimeout(relance.current);
      const essaie = (): void => {
        const attente = REPOS_DEFILEMENT_MS - (Date.now() - dernierDefilement.current);
        if (attente <= 0) {
          relance.current = undefined;
          remonte();
          return;
        }
        relance.current = window.setTimeout(essaie, attente);
      };
      essaie();
    }, [remonte]);

    // 6. Le repère du haut étend la fenêtre, puis déclenche l'historique.
    const plusHaut = vue.precedent !== null || debut > 0;
    useEffect(() => {
      const element = conteneur.current;
      const repere = sentinelle.current;
      if (element === null || repere === null || !plusHaut) return undefined;
      const observateur = new IntersectionObserver(
        (entrees) => {
          if (poseFaite.current && entrees.some((entree) => entree.isIntersecting)) {
            demandeAncien();
          }
        },
        { root: element, rootMargin: `${String(SEUIL_PREMIERES_LIGNES_PX)}px 0px 0px 0px` },
      );
      observateur.observe(repere);
      return () => observateur.disconnect();
    }, [demandeAncien, plusHaut]);

    // 7. Les éléments à l'écran — l'observateur, puis, après chaque rendu, les
    //    nouveaux éléments observés et les disparus oubliés.
    useEffect(() => {
      const racine = conteneur.current;
      const ensemble = aLEcran.current;
      if (racine === null || typeof IntersectionObserver === "undefined") return undefined;
      const observateur = new IntersectionObserver(
        (entrees) => {
          for (const entree of entrees) {
            if (entree.isIntersecting) ensemble.add(entree.target);
            else ensemble.delete(entree.target);
          }
        },
        { root: racine },
      );
      observateurEcran.current = observateur;
      return () => {
        observateur.disconnect();
        observateurEcran.current = null;
        ensemble.clear();
      };
    }, []);
    useEffect(() => {
      const observateur = observateurEcran.current;
      const interieur = contenu.current;
      if (observateur === null || interieur === null) return;
      for (const element of interieur.querySelectorAll(":scope > .fil__el")) {
        observateur.observe(element);
      }
      for (const element of aLEcran.current) {
        if (!element.isConnected) {
          observateur.unobserve(element);
          aLEcran.current.delete(element);
        }
      }
    });

    useEffect(
      () => () => {
        if (enregistrement.current !== undefined) window.clearTimeout(enregistrement.current);
        if (relance.current !== undefined) window.clearTimeout(relance.current);
      },
      [],
    );

    const surDefilement = useCallback(() => {
      const element = conteneur.current;
      if (element === null || !poseFaite.current) return;
      dernierDefilement.current = Date.now();
      suitBas.current = suitLeBas(element);
      setLoinDuBas(!suitBas.current);
      if (suitBas.current) setNouveaux(false);
      if (enregistrement.current !== undefined) window.clearTimeout(enregistrement.current);
      // Retenu au repos : le message en haut de l'écran, pas un ratio.
      enregistrement.current = window.setTimeout(() => {
        enregistrement.current = undefined;
        const interieur = contenu.current;
        if (interieur === null) return;
        const vus = itemsVus.current;
        const haut = element.getBoundingClientRect().top;
        const tous = elementsDuFil(interieur, vus);
        // Seuls les éléments à l'écran se mesurent ; sans observateur (ou
        // avant son premier rapport), tous — le résultat est le même.
        const ecran = aLEcran.current;
        const mesures = ecran.size > 0 ? tous.filter(({ element: el }) => ecran.has(el)) : tous;
        const visible = premierVisible(
          mesures.map(({ cle, element: el }) => {
            const rect = el.getBoundingClientRect();
            return { bas: rect.bottom, cle, haut: rect.top };
          }),
          haut,
        );
        const fin = vus.at(-1);
        ecritPosition(stockageSession(), paneId, {
          ancre: visible?.cle ?? null,
          bas: suitLeBas(element),
          decalage: visible?.decalage ?? 0,
          dernier: fin === undefined ? null : cleDe(fin),
        });
      }, 400);
    }, [paneId]);

    const versLeBas = useCallback(() => {
      const element = conteneur.current;
      if (element === null) return;
      element.scrollTo({ behavior: "smooth", top: element.scrollHeight });
      suitBas.current = true;
      setNouveaux(false);
      setLoinDuBas(false);
    }, []);

    return (
      <div className="fil-cadre">
        <div
          className="fil"
          ref={conteneur}
          onScroll={surDefilement}
          role="log"
          // ⚠️ Muet : chaque jeton était lu à voix haute. `Annonces` dit
          // l'essentiel (état, réponse terminée).
          aria-live="off"
          aria-label="Conversation"
        >
          <div className="fil__contenu" ref={contenu}>
            <span className="fil__sentinelle" ref={sentinelle} aria-hidden="true" />
            {/*
              ⚠️ Un emplacement de hauteur FIXE : le message « Chargement de
              l'historique… » s'insérait au-dessus du contenu et le poussait
              d'une ligne — un saut à chaque page remontée.
            */}
            {items.length > 0 ? (
              <div className="fil__haut">
                {vue.chargementAncien ? (
                  <output className="fil__info">Loading history…</output>
                ) : debut > 0 ? (
                  <button type="button" className="lien fil__info" onClick={etend}>
                    Show earlier messages
                  </button>
                ) : vue.precedent === null ? (
                  <p className="fil__info">Start of the conversation</p>
                ) : null}
              </div>
            ) : null}
            {vue.erreurAncien === null ? null : (
              <p className="fil__info fil__info--erreur" role="alert">
                Unreadable history: {vue.erreurAncien}{" "}
                <button type="button" className="lien" onClick={chargeAncien}>
                  Retry
                </button>
              </p>
            )}
            {items.length > 0 ? null : vue.phase === "chargement" ? (
              <SqueletteFil libelle="Loading the conversation…" />
            ) : vue.phase === "erreur" ? (
              <Avis ton="erreur" annonce="alerte">
                Unreadable conversation: {vue.erreur}
              </Avis>
            ) : envoisEnAttente === 0 ? (
              <div className="fil__vide">
                <strong>No messages</strong>
                <p className="discret">
                  {aUneSession
                    ? "The session is empty for now."
                    : "Herdr does not expose a session for this agent yet: some agents only announce it after their first message."}
                </p>
              </div>
            ) : null}
            {rendus.map((item) => (
              <ElementFil
                key={cleDe(item)}
                item={item}
                enCours={status === "working" && item === dernier}
                cwd={cwd}
                maintenant={jour}
                onOuvrirFichier={onOuvrirFichier}
              />
            ))}
            {children}
          </div>
        </div>
        {/*
          Le retour aux derniers messages : visible dès qu'on n'est plus en bas
          (pas seulement quand du neuf arrive) ; il dit « Nouveaux messages »
          quand il y en a.
        */}
        <Annonces status={status} items={items} />
        {loinDuBas || nouveaux ? (
          <button
            type="button"
            className={nouveaux ? "fil__bas fil__bas--nouveaux" : "fil__bas"}
            onClick={versLeBas}
            aria-label={nouveaux ? "New messages — jump to bottom" : "Jump to latest messages"}
          >
            {nouveaux ? <span>New messages</span> : null}
            <Icone nom="down" petite={nouveaux} />
          </button>
        ) : null}
      </div>
    );
  },
);
Fil.displayName = "Fil";
