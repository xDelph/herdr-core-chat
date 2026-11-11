/**
 * Les ponts entre les magasins et React — `useSyncExternalStore`, rien d'autre.
 */
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import type { EtatAction } from "../actions-locales.ts";
import type { EtatDonnees } from "../donnees.ts";
import type { Envoi } from "../envois.ts";
import { parseRoute, routeToHash, type Route } from "../routing.ts";
import type { PremierMessage } from "../premier-message.ts";
import { actions, envois, magasin, premiers } from "./services.ts";
import { creeSuiviViewport } from "../viewport.ts";

export const useDonnees = (): EtatDonnees =>
  useSyncExternalStore(magasin.souscrire, magasin.etat, magasin.etat);

/**
 * Les envois d'un pane.
 *
 * ⚠️ `liste()` rend un nouveau tableau à chaque appel : le passer tel quel à
 * `useSyncExternalStore` ferait boucler le rendu. On s'abonne à une **version**,
 * et la liste n'est relue que quand elle change.
 */
const versionEnvois = (() => {
  let version = 0;
  envois.souscrire(() => {
    version += 1;
  });
  return () => version;
})();

export const useEnvois = (paneId: string): ReadonlyArray<Envoi> => {
  const version = useSyncExternalStore(envois.souscrire, versionEnvois, versionEnvois);
  const [cache, setCache] = useState<{
    readonly cle: string;
    readonly liste: ReadonlyArray<Envoi>;
  }>(() => ({ cle: `${paneId}:${String(version)}`, liste: envois.liste(paneId) }));
  const cle = `${paneId}:${String(version)}`;
  if (cache.cle !== cle) {
    const suivant = { cle, liste: envois.liste(paneId) };
    setCache(suivant);
    return suivant.liste;
  }
  return cache.liste;
};

/**
 * Tous les envois, de tous les panes — la boîte d'envoi. Même règle que
 * `useEnvois` : relus seulement quand la version change.
 */
export const useTousEnvois = (): ReadonlyArray<Envoi> => {
  const version = useSyncExternalStore(envois.souscrire, versionEnvois, versionEnvois);
  const [cache, setCache] = useState<{
    readonly version: number;
    readonly liste: ReadonlyArray<Envoi>;
  }>(() => ({ liste: envois.tous(), version }));
  if (cache.version !== version) {
    const suivant = { liste: envois.tous(), version };
    setCache(suivant);
    return suivant.liste;
  }
  return cache.liste;
};

export const useAction = (cle: string): EtatAction | undefined => {
  const lit = useCallback(() => actions.lit(cle), [cle]);
  return useSyncExternalStore(actions.souscrire, lit, lit);
};

/** Le routage par fragment : un rechargement garde l'écran. */
export const useRoute = (): readonly [Route, (suivante: Route) => void] => {
  const [route, setRoute] = useState<Route>(() => parseRoute(window.location.hash));
  useEffect(() => {
    const surChangement = () => setRoute(parseRoute(window.location.hash));
    window.addEventListener("hashchange", surChangement);
    return () => window.removeEventListener("hashchange", surChangement);
  }, []);
  const navigue = useCallback((suivante: Route) => {
    const hash = routeToHash(suivante);
    if (window.location.hash !== hash) window.location.hash = hash;
    setRoute(suivante);
  }, []);
  return [route, navigue] as const;
};

/** Une horloge qui ne bat que si on la demande — et pour le seul composant qui l'utilise. */
export const useMaintenant = (intervalle: number | null): number => {
  const [maintenant, setMaintenant] = useState(() => Date.now());
  useEffect(() => {
    if (intervalle === null) return undefined;
    setMaintenant(Date.now());
    const minuterie = window.setInterval(() => setMaintenant(Date.now()), intervalle);
    return () => window.clearInterval(minuterie);
  }, [intervalle]);
  return maintenant;
};

const souscrireReseau = (abonne: () => void): (() => void) => {
  window.addEventListener("online", abonne);
  window.addEventListener("offline", abonne);
  return () => {
    window.removeEventListener("online", abonne);
    window.removeEventListener("offline", abonne);
  };
};
const enLigne = (): boolean => navigator.onLine;
const toujoursEnLigne = (): boolean => true;

/** L'appareil se dit-il en ligne ? (`navigator.onLine`, suivi en direct.) */
export const useEnLigne = (): boolean =>
  useSyncExternalStore(souscrireReseau, enLigne, toujoursEnLigne);

/** La largeur sous laquelle un seul écran est visible (`base.css`). */
const ETROIT = "(max-width: 899px)";
const souscrireEtroit = (abonne: () => void): (() => void) => {
  const requete = window.matchMedia(ETROIT);
  requete.addEventListener("change", abonne);
  return () => requete.removeEventListener("change", abonne);
};
const etroit = (): boolean => window.matchMedia(ETROIT).matches;
const jamaisEtroit = (): boolean => false;

/** Un seul écran à la fois (téléphone) ? */
export const useEtroit = (): boolean => useSyncExternalStore(souscrireEtroit, etroit, jamaisEtroit);

/**
 * Une requête média suivie en direct — `false` hors navigateur (rendu des
 * tests). Sert aux panneaux du bureau : on ne calcule pas ce qu'on ne montre
 * pas.
 */
export const useMedia = (requete: string): boolean =>
  useSyncExternalStore(
    useCallback(
      (abonne: () => void) => {
        if (typeof window === "undefined") return () => undefined;
        const liste = window.matchMedia(requete);
        liste.addEventListener("change", abonne);
        return () => liste.removeEventListener("change", abonne);
      },
      [requete],
    ),
    () => typeof window !== "undefined" && window.matchMedia(requete).matches,
    jamaisEtroit,
  );

/**
 * La hauteur réellement visible, clavier iOS compris.
 *
 * ⚠️ Safari iOS ignore `interactive-widget=resizes-content` : quand le clavier
 * monte, la mise en page garde sa hauteur et le composeur passe sous le
 * clavier. `visualViewport` dit la vérité ; on la pose en variable CSS.
 */
export const useHauteurVisible = (): void => {
  useEffect(() => {
    const vue = window.visualViewport;
    const suivi = creeSuiviViewport();
    let derniere = "";
    const applique = () => {
      /*
        ⚠️ **Le document ne défile jamais — iOS si (retour du 2026-11-10).**
        À l'ouverture du clavier, Safari fait glisser la page entière de la
        hauteur du clavier (`scrollY` 339 mesuré) alors qu'elle ne déborde
        pas. L'app suivait ce glissement par `--haut-visible`, mais les
        touchers ne le suivaient pas : clavier ouvert, Envoyer ne répondait
        plus tant que le champ gardait le focus. On ramène la page en haut :
        l'app occupe alors exactement la zone visible, sans décalage.
      */
      if (window.scrollY !== 0 || window.scrollX !== 0) window.scrollTo(0, 0);
      const mesure = suivi({
        haut: Math.round(vue?.offsetTop ?? 0),
        hauteur: Math.round(vue?.height ?? window.innerHeight),
        largeur: Math.round(vue?.width ?? window.innerWidth),
      });
      const cle = `${String(mesure.hauteur)}:${String(mesure.haut)}:${String(mesure.clavier)}`;
      if (cle === derniere) return;
      derniere = cle;
      const racine = document.documentElement;
      racine.style.setProperty("--hauteur-app", `${String(mesure.hauteur)}px`);
      racine.style.setProperty("--haut-visible", `${String(mesure.haut)}px`);
      racine.classList.toggle("clavier-ouvert", mesure.clavier);
    };
    /*
      ⚠️ **La rotation (retour du 2026-10-29, iPad paysage → portrait).** iOS
      émet `resize` PENDANT la rotation, avec les valeurs de l'orientation
      d'avant, et pas toujours un dernier événement une fois stabilisé : la
      hauteur restait celle du paysage, l'app ne prenait plus tout l'écran.
      On remesure donc aussi quand la page a réellement changé de taille
      (`ResizeObserver`, après la mise en page), sur `orientationchange`, et
      un peu après chaque événement. Une mesure identique ne republie rien.
    */
    const minuteries = new Set<number>();
    const appliqueEtRelit = () => {
      applique();
      for (const delai of [150, 500, 1_000]) {
        const minuterie = window.setTimeout(() => {
          minuteries.delete(minuterie);
          applique();
        }, delai);
        minuteries.add(minuterie);
      }
    };
    const observateur =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(appliqueEtRelit);
    observateur?.observe(document.documentElement);
    applique();
    vue?.addEventListener("resize", applique);
    vue?.addEventListener("scroll", applique);
    window.addEventListener("scroll", applique, { passive: true });
    window.addEventListener("resize", appliqueEtRelit);
    window.addEventListener("orientationchange", appliqueEtRelit);
    return () => {
      observateur?.disconnect();
      for (const minuterie of minuteries) window.clearTimeout(minuterie);
      vue?.removeEventListener("resize", applique);
      vue?.removeEventListener("scroll", applique);
      window.removeEventListener("scroll", applique);
      window.removeEventListener("resize", appliqueEtRelit);
      window.removeEventListener("orientationchange", appliqueEtRelit);
    };
  }, []);
};

/** Le premier message qui attend sur ce pane (lancement), ou rien. */
export const usePremier = (paneId: string): PremierMessage | undefined => {
  useSyncExternalStore(premiers.souscrire, premiers.version, premiers.version);
  return premiers.pour(paneId);
};
