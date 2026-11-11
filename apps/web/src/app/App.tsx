/**
 * L'application : la boîte de réception, la conversation ouverte, les feuilles.
 *
 * Téléphone : un écran à la fois. Large (≥ 900 px) : la liste à gauche, la
 * conversation à droite.
 */
import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import type { AgentView, ConversationSummary } from "@herdr-core-chat/shared";
import { Boite } from "../inbox/Boite.tsx";
import { Rail } from "../inbox/Rail.tsx";
import type { Section } from "../sections.ts";
import { ecritLectures, litLectures, noteLecture, type Lectures } from "../lectures.ts";
import { dossierDe, premierNonVide } from "../libelles.ts";
import { purgeAnciennesPositions, stockageNavigateur } from "../memoire-defilement.ts";
import {
  lireGroupement,
  lireOrdre,
  retenirGroupement,
  retenirOrdre,
  type Groupement,
  type OrdreListe,
} from "../preferences.ts";
import { retiens } from "../pane-ouvert.ts";
import { estAdresseDOnglet, HOME } from "../routing.ts";
import {
  useDonnees,
  useEnLigne,
  useEtroit,
  useHauteurVisible,
  useMaintenant,
  useRoute,
  useTousEnvois,
} from "./hooks.ts";
import { ecrireBrouillon } from "../brouillons.ts";
import { titrePropre } from "../libelles.ts";
import { Notifications } from "../ui/Notifications.tsx";
import { NouvelleVersion } from "../ui/NouvelleVersion.tsx";
import { PleinEcran } from "../ui/PleinEcran.tsx";
import { SqueletteFil } from "../ui/Squelettes.tsx";
import type { PreuvePrompt } from "../envois.ts";
import { envois, magasin, partageLecture, premiers } from "./services.ts";

/**
 * La conversation est chargée **à la demande** : le fil, le Markdown et
 * l'analyse des sessions ne pèsent pas sur la première peinture de la liste,
 * qui est l'écran qu'on ouvre le plus souvent depuis une notification.
 */
const Conversation = lazy(async () => {
  const module = await import("../chat/Conversation.tsx");
  return { default: module.Conversation };
});

/**
 * Les feuilles de lancement et de réglages sont des chemins **froids** : elles
 * s'ouvrent sur un geste, jamais à la première peinture. Chargées à la
 * demande, elles sortent du paquet initial (122,6 Ko gzip mesurés, cible
 * ≤ 120 Ko).
 */
const Lancement = lazy(async () => {
  const module = await import("../feuilles/Lancement.tsx");
  return { default: module.Lancement };
});

const Reglages = lazy(async () => {
  const module = await import("../feuilles/Reglages.tsx");
  return { default: module.Reglages };
});

const BoiteEnvoi = lazy(async () => {
  const module = await import("../feuilles/BoiteEnvoi.tsx");
  return { default: module.BoiteEnvoi };
});

/** Le mur live (`#/mur`) : une tablette posée — chargé seulement là. */
const Mur = lazy(async () => {
  const module = await import("../mur/Mur.tsx");
  return { default: module.Mur };
});

const Connexion = lazy(async () => {
  const module = await import("../feuilles/Connexion.tsx");
  return { default: module.Connexion };
});

/**
 * L'attente du module de conversation (chargé à la demande) : déjà la forme
 * d'un fil, pas un texte au milieu du vide.
 */
const ATTENTE_CONVERSATION = (
  <section className="conversation" aria-busy="true">
    <div className="fil">
      <div className="fil__contenu">
        <SqueletteFil libelle="Opening the conversation…" />
      </div>
    </div>
  </section>
);

type FeuilleApp =
  | { readonly genre: "lancement" }
  | { readonly genre: "reglages" }
  | { readonly genre: "envois" }
  | { readonly genre: "connexion" }
  | null;

/** Le pane et la preuve d'un événement `prompt` — ou rien s'il est hors forme. */
const preuveDe = (donnees: unknown): { pane: string; preuve: PreuvePrompt } | undefined => {
  if (typeof donnees !== "object" || donnees === null) return undefined;
  const pane: unknown = Reflect.get(donnees, "paneId");
  const confirmation: unknown = Reflect.get(donnees, "confirmation");
  const fingerprint: unknown = Reflect.get(donnees, "fingerprint");
  if (typeof pane !== "string" || typeof fingerprint !== "string") return undefined;
  if (confirmation !== "confirmed" && confirmation !== "unknown") return undefined;
  return { pane, preuve: { confirmation, fingerprint } };
};

export const App = () => {
  useHauteurVisible();
  const donnees = useDonnees();
  const [route, navigue] = useRoute();
  const etroit = useEtroit();
  const [groupement, setGroupement] = useState<Groupement>(lireGroupement);
  const [ordre, setOrdre] = useState<OrdreListe>(lireOrdre);
  const [lectures, setLectures] = useState<Lectures>(() => litLectures(stockageNavigateur()));
  const [feuille, setFeuille] = useState<FeuilleApp>(null);
  /** Le filtre d'état du rail (bureau) ; `null` : toute la liste. */
  const [filtreSection, setFiltreSection] = useState<Section | null>(null);
  /**
   * Le dernier état connu de chaque pane ouvert — pour les **libellés**
   * seulement (titre, projet, agent). ⚠️ Jamais pour l'état : un pane fermé
   * restait « Travaille » avec Arrêter actif (`pane-ouvert.ts`). Bornée.
   */
  const [memoire, setMemoire] = useState<ReadonlyMap<string, AgentView>>(new Map());

  useEffect(() => {
    // Une ancienne adresse d'onglet (#/reglages…) retombe sur la liste.
    if (estAdresseDOnglet(window.location.hash)) window.location.replace(HOME);
    // L'ancienne mémoire de défilement (ratios gardés pour toujours) part.
    purgeAnciennesPositions(globalThis.localStorage);
    magasin.demarre();
    // ⚠️ La remise d'un envoi est prouvée par l'ÉVÉNEMENT `prompt` du démon,
    // pas par l'absence d'erreur — pour le pane ET l'empreinte qu'il nomme.
    const arrete = magasin.ecoute((evenement) => {
      if (evenement.genre !== "prompt") return;
      const lu = preuveDe(evenement.donnees);
      if (lu !== undefined) envois.resoutParEvenement(lu.pane, lu.preuve);
    });
    return () => {
      arrete();
      magasin.arrete();
    };
  }, []);

  useEffect(() => {
    /*
      Au premier plan : relire tout de suite, puis encore une fois un peu
      après. ⚠️ Sorti de veille, l'iPhone n'a pas toujours le réseau à la
      première milliseconde : la lecture immédiate échouait, et l'écran
      gardait l'état d'avant la veille jusqu'au filet de 30 s (retour du
      2026-11-11). `pageshow` couvre la page rendue depuis le cache.
    */
    const minuteries = new Set<number>();
    const auPremierPlan = () => {
      if (document.visibilityState !== "visible") return;
      void magasin.rafraichit(true);
      const minuterie = window.setTimeout(() => {
        minuteries.delete(minuterie);
        void magasin.rafraichit(true);
      }, 1_500);
      minuteries.add(minuterie);
    };
    document.addEventListener("visibilitychange", auPremierPlan);
    window.addEventListener("pageshow", auPremierPlan);
    window.addEventListener("online", auPremierPlan);
    return () => {
      for (const minuterie of minuteries) window.clearTimeout(minuterie);
      document.removeEventListener("visibilitychange", auPremierPlan);
      window.removeEventListener("pageshow", auPremierPlan);
      window.removeEventListener("online", auPremierPlan);
    };
  }, []);

  const agents = useMemo(() => donnees.overview?.agents ?? [], [donnees.overview]);
  const paneOuvert = route.kind === "agent" ? route.paneId : null;
  const agentLu =
    paneOuvert === null ? undefined : agents.find((agent) => agent.paneId === paneOuvert);

  // Le premier message d'un agent lancé d'ici part quand l'agent est prêt
  // (`premier-message.ts`) — par les envois, avec leurs preuves et leurs états.
  useEffect(() => {
    for (const message of premiers.prets(agents)) {
      envois.ajoute({ apres: null, creeA: message.creeA, pane: message.pane, text: message.texte });
    }
  }, [agents]);

  useEffect(() => {
    if (agentLu === undefined) return;
    setMemoire((precedente) => retiens(precedente, agentLu.paneId, agentLu));
  }, [agentLu]);

  const changeGroupement = useCallback((suivant: Groupement) => {
    retenirGroupement(suivant);
    setGroupement(suivant);
  }, []);

  const changeOrdre = useCallback((suivant: OrdreListe) => {
    retenirOrdre(suivant);
    setOrdre(suivant);
  }, []);

  const noteVue = useCallback((paneId: string, instant: number) => {
    setLectures((precedentes) => {
      const suivantes = noteLecture(precedentes, paneId, instant);
      if (suivantes !== precedentes) ecritLectures(stockageNavigateur(), suivantes);
      return suivantes;
    });
    partageLecture(paneId, instant);
  }, []);

  /** « Mark all read » : chaque réponse finie, lue maintenant (et partagée). */
  const toutLire = useCallback(
    (lignes: ReadonlyArray<ConversationSummary>) => {
      const instant = Date.now();
      for (const ligne of lignes) noteVue(ligne.paneId, instant);
    },
    [noteVue],
  );

  const retour = useCallback(() => navigue({ kind: "liste" }), [navigue]);
  // Stables : la boîte est mémoïsée, une flèche neuve à chaque rendu la
  // redessinerait pour rien.
  const ouvreLancement = useCallback(() => setFeuille({ genre: "lancement" }), []);
  const ouvreReglages = useCallback(() => setFeuille({ genre: "reglages" }), []);
  const ouvreBoiteEnvoi = useCallback(() => setFeuille({ genre: "envois" }), []);
  const ouvreConnexion = useCallback(() => setFeuille({ genre: "connexion" }), []);
  const tousEnvois = useTousEnvois();
  const enLigne = useEnLigne();
  // L'horloge des feuilles « âge » : seulement quand l'une d'elles est ouverte.
  const maintenant = useMaintenant(
    feuille?.genre === "envois" || feuille?.genre === "connexion" ? 1_000 : null,
  );
  /** Le nom d'un pane pour la boîte d'envoi : la ligne lue, sinon l'overview. */
  const titreDe = useCallback(
    (pane: string): string => {
      const ligne = donnees.conversations?.find((candidat) => candidat.paneId === pane);
      if (ligne !== undefined) return titrePropre(ligne.title);
      const vu = donnees.overview?.agents.find((candidat) => candidat.paneId === pane);
      return premierNonVide(vu?.name, vu?.workspaceLabel, pane);
    },
    [donnees.conversations, donnees.overview],
  );
  const reessaie = useCallback(() => void magasin.rafraichit(true), []);

  // Les libellés et les voisins lisent le vivant, sinon le souvenir ; l'état et
  // les gestes de la conversation ne lisent que le vivant (`agentLu`).
  const souvenir = paneOuvert === null ? undefined : memoire.get(paneOuvert);
  const agent = agentLu ?? souvenir ?? null;
  const conversation =
    paneOuvert === null
      ? null
      : (donnees.conversations?.find((ligne) => ligne.paneId === paneOuvert) ?? null);
  const espace = donnees.overview?.workspaces.find(
    (candidat) => candidat.workspaceId === (agent?.workspaceId ?? conversation?.workspaceId),
  );
  const projet = premierNonVide(
    espace?.label,
    dossierDe(espace?.checkoutPath),
    agent?.workspaceLabel,
    dossierDe(agent?.cwd),
    agent?.workspaceId ?? conversation?.workspaceId,
  );
  /*
    Les panes du même onglet, **dans l'ordre de la disposition** (`paneRank`)
    — celui où ils apparaissent sur le Mac, comme dans l'ancienne app. L'ordre
    de l'overview n'est pas une disposition.
  */
  const voisins = useMemo(
    () =>
      agent === null
        ? []
        : agents
            .filter((candidat) => candidat.tabId === agent.tabId)
            .toSorted(
              (a, b) =>
                (a.paneRank ?? Number.MAX_SAFE_INTEGER) - (b.paneRank ?? Number.MAX_SAFE_INTEGER) ||
                a.paneId.localeCompare(b.paneId),
            ),
    [agent, agents],
  );

  if (route.kind === "mur") {
    return (
      <div className="app" data-vue="mur">
        <Notifications />
        <NouvelleVersion />
        <Suspense fallback={null}>
          <Mur donnees={donnees} lectures={lectures} onQuitter={retour} />
        </Suspense>
      </div>
    );
  }

  return (
    <div className="app" data-vue={route.kind === "agent" ? "agent" : "liste"}>
      <div className="barre-etat" aria-hidden="true" />
      <Notifications />
      <NouvelleVersion />
      <PleinEcran />
      <Rail
        donnees={donnees}
        lectures={lectures}
        filtre={filtreSection}
        projetCourant={route.kind === "projet" ? route.workspaceId : null}
        onFiltre={setFiltreSection}
        onLancer={ouvreLancement}
        onBoiteEnvoi={ouvreBoiteEnvoi}
        onReglages={ouvreReglages}
        onConnexion={ouvreConnexion}
      />
      <Boite
        donnees={donnees}
        lectures={lectures}
        groupement={groupement}
        ordre={ordre}
        projet={route.kind === "projet" ? route.workspaceId : null}
        filtreSection={filtreSection}
        onToutLire={toutLire}
        onLancer={ouvreLancement}
        onReglages={ouvreReglages}
        onConnexion={ouvreConnexion}
        onBoiteEnvoi={ouvreBoiteEnvoi}
        onReessayer={reessaie}
        paneCourant={paneOuvert}
        cachee={etroit && paneOuvert !== null}
      />
      {paneOuvert === null ? (
        <div className="vide-large">
          <p>Choose a conversation, or launch an agent with +.</p>
        </div>
      ) : (
        <Suspense fallback={ATTENTE_CONVERSATION}>
          <Conversation
            key={paneOuvert}
            paneId={paneOuvert}
            agent={agentLu ?? null}
            souvenir={souvenir}
            overviewLu={donnees.overview !== null}
            vuAvant={souvenir !== undefined}
            conversation={conversation}
            projet={projet}
            voisins={voisins}
            debutObserve={donnees.debutsTravail.get(paneOuvert)}
            lueA={donnees.dernierSucces}
            onRetour={retour}
            onVue={noteVue}
          />
        </Suspense>
      )}
      {feuille?.genre === "lancement" ? (
        <Suspense fallback={null}>
          <Lancement
            espaces={donnees.overview?.workspaces ?? []}
            kinds={donnees.kinds}
            erreurKinds={donnees.erreurKinds}
            espaceInitial={
              route.kind === "projet" ? route.workspaceId : (agent?.workspaceId ?? null)
            }
            onClose={() => setFeuille(null)}
            onLance={(paneId, premier) => {
              setFeuille(null);
              premiers.attend(paneId, premier, Date.now());
              navigue({ kind: "agent", paneId });
            }}
          />
        </Suspense>
      ) : null}
      {feuille?.genre === "envois" ? (
        <Suspense fallback={null}>
          <BoiteEnvoi
            envois={tousEnvois}
            magasin={envois}
            titreDe={titreDe}
            maintenant={maintenant}
            paneCourant={paneOuvert}
            onOuvrir={(pane) => {
              setFeuille(null);
              navigue({ kind: "agent", paneId: pane });
            }}
            onModifier={(pane, texte) => {
              ecrireBrouillon(pane, texte);
              setFeuille(null);
              navigue({ kind: "agent", paneId: pane });
            }}
            onClose={() => setFeuille(null)}
          />
        </Suspense>
      ) : null}
      {feuille?.genre === "connexion" ? (
        <Suspense fallback={null}>
          <Connexion
            donnees={donnees}
            enLigne={enLigne}
            derniereVie={magasin.derniereVie()}
            maintenant={maintenant}
            onReessayer={reessaie}
            onClose={() => setFeuille(null)}
          />
        </Suspense>
      ) : null}
      {feuille?.genre === "reglages" ? (
        <Suspense fallback={null}>
          <Reglages
            health={donnees.health}
            erreurHealth={donnees.erreurs.health}
            overview={donnees.overview}
            groupement={groupement}
            onGroupement={changeGroupement}
            ordre={ordre}
            onOrdre={changeOrdre}
            onClose={() => setFeuille(null)}
          />
        </Suspense>
      ) : null}
    </div>
  );
};
