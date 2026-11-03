//! Le démon herdr-core-chat (T056).
//!
//! ⚠️ **Ce binaire refuse de démarrer à moitié.** Quatre ports sur cinq sont
//! composés (contrôle Herdr, catalogues, artefacts, notifications) ; `sessions`
//! manque encore, et c'est le gros morceau (lecteurs par famille, curseurs,
//! bornage). Plutôt que de servir des conversations inventées, le démon
//! s'arrête avec la liste de ce qui manque — un `main` qui « marche » sans
//! adaptateur serait le mensonge le plus coûteux de la migration.
//!
//! La **composition** (`composition::services`) et le **cycle de vie**
//! (`cycle::sert`) sont, eux, complets et testés : dès que les adaptateurs
//! existent, `main` les branche ici, sans rien réécrire.

use herdr_core_chat::config::{Config, config_depuis};
use herdr_core_chat::cycle::{Arret, Declencheur, ErreurDemarrage};
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // `--version` : la seule commande qui ne démarre rien. Elle existe pour que
    // l'installation (formule Homebrew, service) puisse vérifier le binaire
    // sans lui demander d'écouter sur un port — et sans Bun, sans Node.
    if std::env::args().any(|argument| argument == "--version") {
        println!("herdr-core-chat {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let environnement: HashMap<String, String> = std::env::vars().collect();
    /*
      ⚠️ **Sans abonné, chaque `tracing::warn!` était perdu.** Les dégradations
      de flux, les pannes SQLite, les livraisons push échouées étaient écrites…
      vers personne : `tracing-subscriber` n'était jamais initialisé. Les traces
      vont sur `stderr` (que le service redirige vers `out.log`), filtrées par
      `RUST_LOG`, `info` par défaut.
    */
    if let Err(cause) = herdr_core_chat::logging::initialise_traces(
        environnement.get("RUST_LOG").map(String::as_str),
    ) {
        eprintln!("{cause}");
    }
    let maison = environnement
        .get("HOME")
        .cloned()
        .unwrap_or_else(|| String::from("/"));
    let config = match config_depuis(&environnement, std::path::Path::new(&maison)) {
        Ok(config) => config,
        Err(erreur) => {
            eprintln!("configuration invalide : {}", erreur.detail);
            std::process::exit(1);
        }
    };
    if let Err(cause) = demarre(&config).await {
        eprintln!("{cause}");
        std::process::exit(1);
    }
}

/// Le démarrage — refusé tant que les adaptateurs ne sont pas composés.
///
/// Le refus est **nomme** : il dit exactement quels ports manquent, et il ne
/// cree aucun socket, n'ecrit aucun journal, ne lance aucun worker.
async fn demarre(config: &Config) -> Result<(), String> {
    // 1) Le contrôle : l'adaptateur herdr-core (une connexion par demande)
    //    et l'état publié par Herdr.app pour les noms d'espaces et d'onglets.
    let socket_path = config.herdr_socket_path.clone();
    let controle = Arc::new(herdr_core_client::port::PortCore::nouveau(
        herdr_core_client::client::ClientCore::nouveau(&socket_path),
        &config.etat_herdr_app_path,
    ));
    // Le port de contrôle vu par les handlers : le même objet, derrière son trait.
    let controle_port: Arc<dyn application::ports::ControlPort> = Arc::clone(&controle) as _;
    // Adaptateur herdr-core non implémenté (P0/P2) : aucune intégration amont.
    let integrations: catalog::port::SourceIntegrations =
        Arc::new(move || Err(String::from("adaptateur herdr-core non implémenté (P2)")));
    // UN cache de catalogues, partagé par `/api/agents/:kind/models` et la
    // validation du lancement (le niveau de raisonnement y est vérifié).
    let cache_catalogues = Arc::new(catalog::cache::CacheCatalogues::nouveau());
    let catalogue: Arc<dyn application::ports::CatalogPort> = Arc::new(
        catalog::port::PortCatalogue::nouveau(Arc::clone(&cache_catalogues), integrations),
    );
    // 3) Les artefacts : confinés aux racines de CHAQUE requête.
    let artefacts = Arc::new(files::port::PortArtefacts::nouveau());
    // 4) Les notifications : notre store (`~/.herdr-core-chat/push.json`),
    //    jamais celui d'Herdr Chat, et les clés VAPID de l'environnement
    //    (E08) composés par `assemblage::store_push`.
    let store_push = herdr_core_chat::assemblage::store_push(config)?;
    // UNE livraison réelle, partagée par le port (H15/H16) et le notifieur de
    // la veille — le `sub` du JWT est le sujet configuré.
    let livraison =
        notifications::port::livraison_reelle(&store_push, config.vapid_sujet_resolu.clone());
    let notifications = Arc::new(notifications::port::PortNotifications::avec_livraison(
        Arc::clone(&store_push),
        Arc::clone(&livraison),
    ));
    // La présence (H23) : le port l'enregistre, le notifieur l'écarte — le
    // MÊME registre, sinon l'appareil au premier plan serait encore notifié.
    let notifieur = Arc::new(
        notifications::notifieur::Notifieur::nouveau(livraison, Arc::clone(&store_push))
            .avec_presence(notifications.presence()),
    );
    // 5) Les sessions : les racines de la configuration, lues par les
    //    lecteurs par famille.
    let port_sessions: Arc<dyn application::ports::SessionPort> =
        Arc::new(sessions::port::PortSessions::nouveau(
            herdr_core_chat::assemblage::racines_de_sessions(config),
        ));

    // 6) Les Services du routeur : les ports entrent, le monde sort.
    let maintenant: Arc<dyn application::refresh::SourceMaintenant> =
        Arc::new(application::refresh::MaintenantReelle);
    let ports = herdr_core_chat::composition::Ports {
        controle: Arc::clone(&controle_port),
        sessions: Arc::clone(&port_sessions),
        observateur: Arc::new(herdr_core_chat::assemblage::ObservateurViaHub {
            hub: sessions::hub::SessionHub::nouveau(),
        }),
        catalogue: Arc::clone(&catalogue),
        artefacts: Arc::clone(&artefacts) as Arc<dyn application::ports::ArtifactPort>,
        notifications: Some(
            Arc::clone(&notifications) as Arc<dyn application::ports::NotificationPort>
        ),
        resout: Arc::new(herdr_core_chat::assemblage::ResoutSessionsViaPort {
            port: Arc::clone(&port_sessions),
        }),
        lit_sessions: Arc::new(herdr_core_chat::assemblage::LitSessionsViaPort {
            port: Arc::clone(&port_sessions),
            ouvertures: Arc::default(),
            familles: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            fenetres: Arc::new(std::sync::Mutex::new(
                herdr_core_chat::assemblage::CacheFenetres::default(),
            )),
            taches: herdr_core_chat::assemblage::lecteur_de_taches(config),
        }),
        types: Arc::new(herdr_core_chat::assemblage::TypesViaCatalogue {
            port: Arc::clone(&catalogue),
        }),
        branche: Arc::new(herdr_core_chat::assemblage::lecteur_de_branches(
            Arc::clone(&maintenant),
        )),
        // La validation des args ET du niveau de raisonnement contre le
        // catalogue mesuré — refusée AVANT tout onglet (C07).
        args: Arc::new(catalog::args::verificateur_args(Arc::clone(
            &cache_catalogues,
        ))),
    };
    // 7) Le journal des diagnostics : le vrai, à côté des données.
    let diagnostics = Arc::new(herdr_core_chat::logging::Journal::nouveau(
        config.log_path.clone(),
    ));
    let services = Arc::new(herdr_core_chat::composition::services(
        config,
        ports,
        herdr_core_chat::composition::direct(),
        diagnostics,
        Arc::new(herdr_core_chat::assets::AssetsEmbarques),
    ));
    /*
      ⚠️ **La veille démarre AVANT le routeur, et elle manquait totalement.**
      Sans elle, le démon ne publiait jamais `agent_status` ni
      `session_changed` : la PWA retombait sur son filet (30 s / 120 s), donc
      ni statut en direct, ni message en direct — même écran ouvert.
    */
    let mut veille = herdr_core_chat::veille::Veille::demarre(
        Arc::clone(&services),
        Arc::clone(&maintenant),
        Some(notifieur),
    );
    // Le ping de démarrage (O03) : à côté, jamais devant l'écoute — un Herdr
    // injoignable est un avertissement dans les traces, pas un démon muet.
    let _ping = herdr_core_chat::demarrage::lance_ping_de_demarrage(Arc::clone(&controle_port));
    // 8) Le routeur, l'écoute et le cycle de vie.
    let adresse = format!("{}:{}", config.hostname, config.port);
    let adresse: std::net::SocketAddr = adresse
        .parse()
        .map_err(|cause| format!("adresse invalide ({adresse}): {cause}"))?;
    let ecouteur = herdr_core_chat::cycle::ecoute(adresse)
        .await
        .map_err(|erreur| erreur.to_string())?;
    let (declencheur, arret) = herdr_core_chat::cycle::Declencheur::nouveau();
    // ⚠️ Le déclencheur est DÉPLACÉ dans la tâche de signaux : il n'est pas
    // clonable, et c'est voulu (un seul point demande l'arrêt).
    tokio::spawn(async move {
        herdr_core_chat::cycle::signaux().await;
        declencheur.demande();
    });
    // ⚠️ Les origines Tailscale lues au démarrage peuvent manquer (Tailscale
    // pas encore prêt à l'ouverture de session) : la garde les redécouvre
    // quand une origine inconnue se présente (`OriginesAutorisees`).
    let port_public = config.port;
    let origines = http_api::origin::OriginesAutorisees::avec_redecouverte(
        config.origines_publiques.clone(),
        Arc::new(move || herdr_core_chat::config::origines_tailscale(port_public)),
        http_api::origin::INTERVALLE_REDECOUVERTE,
    );
    let routeur = http_api::router::routeur(services, origines);
    let rapport = herdr_core_chat::cycle::sert(
        ecouteur,
        routeur,
        Arc::new(herdr_core_chat::cycle::EnCours::nouveau()),
        Arc::new(herdr_core_chat::cycle::Taches::nouveau()),
        Arc::new(herdr_core_chat::logging::Journal::nouveau(
            config.log_path.clone(),
        )),
        arret,
        herdr_core_chat::cycle::DELAI_DRAIN,
    )
    .await;
    // La veille s'arrête avec le serveur : un fil qui interroge Herdr après la
    // fermeture du socket d'écoute serait un résidu.
    veille.arrete();
    if rapport.drain_interrompu {
        eprintln!(
            "arrêt : {} requête(s) coupée(s) après le délai de drain",
            rapport.requetes_coupees
        );
    }
    Ok(())
}
/// Les elements du cycle de vie, exposes pour que le cablage final (quand les
/// adaptateurs existeront) n'ait qu'a les appeler — et pour que les tests
/// puissent les verifier sans binaire.
#[allow(dead_code)]
pub struct Cycle {
    pub declencheur: Declencheur,
    pub arret: Arret,
}

/// Prepare le cycle : l'arret est declenche par `SIGINT`/`SIGTERM`.
#[allow(dead_code)]
pub fn prepare_cycle() -> Cycle {
    let (declencheur, arret) = Declencheur::nouveau();
    Cycle { declencheur, arret }
}

/// Les erreurs de démarrage que `main` sait traduire en message utilisateur.
#[allow(dead_code)]
pub fn decrit(erreur: &ErreurDemarrage) -> String {
    erreur.to_string()
}
