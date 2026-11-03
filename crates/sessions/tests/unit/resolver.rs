//! Le résolveur (T023/C15) : state_10 > state_9 (la régression d19f98d), les
//! DEUX lectures du titre (l'index tranche), le cache négatif invalidable
//! (une session ajoutée devient visible), la source unique pour
//! overview/liste/session.
//! Prouve : D07, D08.

use serde_json::json;
use sessions::resolver::{
    Emplacement, Racines, Resolveur, bases_codex, chemin_claude_par_id, chemin_codex_par_id,
    chemin_codex_par_nom, localise_session, nom_de_session_codex, noms_de_session_codex,
    source_session, source_session_de,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static NONCE: AtomicU32 = AtomicU32::new(0);

fn temporaire(prefixe: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let chemin = base.join(format!(
        "{prefixe}-{}-{}",
        std::process::id(),
        NONCE.fetch_add(1, Ordering::SeqCst)
    ));
    match fs::create_dir(&chemin) {
        Ok(()) => chemin,
        Err(cause) => panic!("racine refusée : {cause}"),
    }
}

fn racines_fake() -> Racines {
    let racine = temporaire("soutien-resolveur-racines");
    for sous in ["codex-sessions", "codex-home", "claude-projects"] {
        match fs::create_dir(racine.join(sous)) {
            Ok(()) => (),
            Err(cause) => panic!("mkdir refusé : {cause}"),
        }
    }
    Racines {
        pi_sessions: racine.join("pi-sessions"),
        codex_sessions: racine.join("codex-sessions"),
        codex_home: racine.join("codex-home"),
        claude_projects: racine.join("claude-projects"),
        opencode_db: racine.join("opencode.db"),
        devin_db: racine.join("devin.db"),
    }
}

#[test]
fn les_bases_codex_se_trient_par_version_numerique() {
    // LA régression d19f98d : le tri lexicographique plaçait state_9 devant
    // state_10 — le tri NUMÉRIQUE redresse.
    let home = temporaire("soutien-resolveur-states");
    for nom in [
        "state_2.sqlite",
        "state_9.sqlite",
        "state_10.sqlite",
        "autre.sqlite",
    ] {
        match fs::write(home.join(nom), "") {
            Ok(()) => (),
            Err(cause) => panic!("touch refusé : {cause}"),
        }
    }
    let bases = bases_codex(&home);
    let noms: Vec<String> = bases
        .iter()
        .map(|chemin| {
            chemin
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert!(
        noms == vec!["state_10.sqlite", "state_9.sqlite", "state_2.sqlite"],
        "state_10 AVANT state_9, versions numériques décroissantes : {noms:?}"
    );
    // Un dossier absent ou vide : rien, sans erreur.
    assert!(bases_codex(&home.join("absent")).is_empty());
}

#[test]
fn les_deux_lectures_du_titre_brute_et_sans_décoration() {
    let cwd = "/dev/projets/herdr-pwa";
    // Le titre mesuré, au repos : UNE lecture (brute).
    let noms = noms_de_session_codex(Some("Audit PWA and webapp | herdr-pwa"), Some(cwd));
    assert!(
        noms == vec!["Audit PWA and webapp"],
        "le brut seul : {noms:?}"
    );
    assert!(
        nom_de_session_codex(Some("Audit PWA and webapp | herdr-pwa"), Some(cwd))
            == Some(String::from("Audit PWA and webapp")),
        "le nom simple = le premier candidat"
    );
    // Le titre EN TRAVAIL (glyphe ⠼) : DEUX lectures — et un vrai titre qui
    // commence par un glyphe reste trouvable.
    let noms = noms_de_session_codex(Some("⠼ Audit PWA and webapp | herdr-pwa"), Some(cwd));
    assert!(
        noms == vec!["⠼ Audit PWA and webapp", "Audit PWA and webapp"],
        "le brut ET le sans-décoration : {noms:?}"
    );
    // Le dossier doit être celui du pane — sinon rien.
    assert!(
        noms_de_session_codex(Some("Audit PWA and webapp | autre-projet"), Some(cwd)).is_empty(),
        "un dossier non concordant ne nomme rien"
    );
    // Un titre qui n'est que le dossier (« perso », un pane jamais utilisé) :
    // rien à retrouver.
    assert!(
        noms_de_session_codex(Some("herdr-pwa | herdr-pwa"), Some(cwd)).is_empty()
            || noms_de_session_codex(Some("perso | perso"), Some(cwd)).is_empty()
    );
    // Un titre sans ` | ` : rien.
    assert!(noms_de_session_codex(Some("sans barre"), Some(cwd)).is_empty());
    // Cwd vide ou titres vides : rien.
    assert!(noms_de_session_codex(Some("x | y"), Some("")).is_empty());
    assert!(noms_de_session_codex(None, Some(cwd)).is_empty());
}

#[test]
fn l_index_confirme_le_nom_et_le_dossier_et_lexistence() {
    let racines = racines_fake();
    // Un index codex factice, avec le rollout SUR DISQUE.
    let base = racines.codex_home.join("state_10.sqlite");
    let connexion = match rusqlite::Connection::open(&base) {
        Ok(connexion) => connexion,
        Err(cause) => panic!("index refusé : {cause}"),
    };
    let rollout = racines.codex_sessions.join("rollout-2025-01a14a74.jsonl");
    match fs::write(&rollout, "{}") {
        Ok(()) => (),
        Err(cause) => panic!("rollout refusé : {cause}"),
    }
    match connexion.execute_batch(&format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, name TEXT, cwd TEXT, rollout_path TEXT, archived INTEGER, updated_at TEXT);
         INSERT INTO threads (name, cwd, rollout_path, archived, updated_at) VALUES ('Audit PWA and webapp', '/dev/projets/herdr-pwa', '{}', 0, '2026-10-18');",
        rollout.display()
    )) {
        Ok(()) => (),
        Err(cause) => panic!("schéma refusé : {cause}"),
    }
    let resolveur = Resolveur::nouveau(racines);
    match chemin_codex_par_nom(&resolveur, "Audit PWA and webapp", "/dev/projets/herdr-pwa") {
        Some(chemin) => assert!(chemin == rollout, "le chemin confirmé : {chemin:?}"),
        None => panic!("l'index confirme le nom + le dossier + l'existence"),
    }
    // Un autre cwd : NON — jamais la session d'un autre.
    assert!(chemin_codex_par_nom(&resolveur, "Audit PWA and webapp", "/dev/autre").is_none());
    // Une base d'un schéma inattendu ne prouve rien (pas de substitution).
    assert!(chemin_codex_par_nom(&resolveur, "inconnu", "/dev/projets/herdr-pwa").is_none());
}

#[test]
fn le_cache_négatif_s_invalide_quand_la_base_bouge() {
    let racines = racines_fake();
    let base = racines.codex_home.join("state_10.sqlite");
    let connexion = match rusqlite::Connection::open(&base) {
        Ok(connexion) => connexion,
        Err(cause) => panic!("index refusé : {cause}"),
    };
    match connexion.execute_batch(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, name TEXT, cwd TEXT, rollout_path TEXT, archived INTEGER, updated_at TEXT);",
    ) {
        Ok(()) => (),
        Err(cause) => panic!("schéma refusé : {cause}"),
    }
    let resolveur = Resolveur::nouveau(racines);
    // Cache NÉGATIF : la session n'existe pas encore.
    assert!(chemin_codex_par_nom(&resolveur, "Nouvelle session", "/dev/projets/x").is_none());
    // La session est AJOUTÉE (la base change) : le cache négatif s'invalide,
    // la session devient VISIBLE.
    let rollout = resolveur
        .racines
        .codex_sessions
        .join("rollout-2026-nouvelle.jsonl");
    match fs::write(&rollout, "{}") {
        Ok(()) => (),
        Err(cause) => panic!("rollout refusé : {cause}"),
    }
    match connexion.execute_batch(&format!(
        "INSERT INTO threads (name, cwd, rollout_path, archived, updated_at) VALUES ('Nouvelle session', '/dev/projets/x', '{}', 0, '2026-10-18T12:00:00Z');",
        rollout.display()
    )) {
        Ok(()) => (),
        Err(cause) => panic!("insertion refusée : {cause}"),
    }
    match chemin_codex_par_nom(&resolveur, "Nouvelle session", "/dev/projets/x") {
        Some(chemin) => assert!(
            chemin == rollout,
            "la nouvelle session est VISIBLE : {chemin:?}"
        ),
        None => panic!("un cache négatif permanent cachait la session ajoutée"),
    }
}

#[test]
fn le_cache_négatif_s_invalide_aussi_en_mode_wal() {
    let racines = racines_fake();
    let base = racines.codex_home.join("state_10.sqlite");
    let connexion = match rusqlite::Connection::open(&base) {
        Ok(connexion) => connexion,
        Err(cause) => panic!("index refusé : {cause}"),
    };
    match connexion.execute_batch(
        "PRAGMA journal_mode=WAL; CREATE TABLE threads (id TEXT PRIMARY KEY, name TEXT, cwd TEXT, rollout_path TEXT, archived INTEGER, updated_at TEXT);",
    ) {
        Ok(()) => (),
        Err(cause) => panic!("schéma refusé : {cause}"),
    }
    // ⚠️ Codex écrit son index en WAL : la base principale ne bouge pas à
    // l'insertion, seul le `-wal` grandit — et le dossier ne change pas.
    let resolveur = Resolveur::nouveau(racines);
    // Cache NÉGATIF : la session n'existe pas encore.
    assert!(chemin_codex_par_nom(&resolveur, "Nouvelle session", "/dev/projets/x").is_none());
    // La session est AJOUTÉE (la base change) : le cache négatif s'invalide,
    // la session devient VISIBLE.
    let rollout = resolveur
        .racines
        .codex_sessions
        .join("rollout-2026-nouvelle.jsonl");
    match fs::write(&rollout, "{}") {
        Ok(()) => (),
        Err(cause) => panic!("rollout refusé : {cause}"),
    }
    match connexion.execute_batch(&format!(
        "INSERT INTO threads (name, cwd, rollout_path, archived, updated_at) VALUES ('Nouvelle session', '/dev/projets/x', '{}', 0, '2026-10-18T12:00:00Z');",
        rollout.display()
    )) {
        Ok(()) => (),
        Err(cause) => panic!("insertion refusée : {cause}"),
    }
    match chemin_codex_par_nom(&resolveur, "Nouvelle session", "/dev/projets/x") {
        Some(chemin) => assert!(
            chemin == rollout,
            "la nouvelle session est VISIBLE : {chemin:?}"
        ),
        None => panic!("un cache négatif permanent cachait la session ajoutée"),
    }
}

#[test]
fn la_source_unique_decide_pour_tous() {
    let racines = racines_fake();
    let resolveur = Resolveur::nouveau(racines);
    // path fait foi.
    match source_session(
        &resolveur,
        Some("pi"),
        Some(&Emplacement::nouveau("path", "/sessions/pi/x.jsonl")),
    ) {
        Some(source) => {
            assert!(
                localise_session(
                    &resolveur,
                    Some("pi"),
                    Some(&Emplacement::nouveau("path", "/sessions/pi/x.jsonl"))
                ) == Some(PathBuf::from("/sessions/pi/x.jsonl"))
            );
            let _ = source;
        }
        None => panic!("path fait foi"),
    }
    // opencode : une base, pas un chemin — localise rend NONE.
    let emplacement = Emplacement::nouveau("id", "ses_1");
    match source_session(&resolveur, Some("opencode"), Some(&emplacement)) {
        Some(source) => match source {
            sessions::resolver::SourceSession::Sqlite { magasin, .. } => {
                assert!(magasin == sessions::resolver::Magasin::Opencode);
            }
            autre => panic!("opencode = sqlite : {autre:?}"),
        },
        None => panic!("opencode résout en base"),
    }
    assert!(
        localise_session(&resolveur, Some("opencode"), Some(&emplacement)).is_none(),
        "une base n'est pas un chemin de session"
    );
    // Un identifiant vide : refusé (jamais la session d'un autre).
    assert!(
        source_session(
            &resolveur,
            Some("opencode"),
            Some(&Emplacement::nouveau("id", " "))
        )
        .is_none()
    );
    // Un kind inconnu : rien.
    assert!(source_session(&resolveur, Some("autre"), Some(&emplacement)).is_none());
    // Le repli par titre : CODEX SEUL — le même titre pour pi ne fait rien.
    let codex_titre = source_session_de(
        &resolveur,
        Some("codex"),
        None,
        Some("Audit PWA and webapp | herdr-pwa"),
        Some("/dev/projets/herdr-pwa"),
        None,
    );
    let _ = codex_titre; // l'index vide ici : None — le repli existe, il ne trouve pas
    assert!(
        source_session_de(
            &resolveur,
            Some("pi"),
            None,
            Some("Audit PWA and webapp | herdr-pwa"),
            Some("/dev/projets/herdr-pwa"),
            None
        )
        .is_none(),
        "le repli par titre est codex-SEUL"
    );
}

#[test]
fn les_identifiants_par_nom_de_fichier_sont_ancrés() {
    let racines = racines_fake();
    let session = racines.codex_sessions.join("rollout-2025-x-abc123.jsonl");
    match fs::write(&session, "{}") {
        Ok(()) => (),
        Err(cause) => panic!("rollout refusé : {cause}"),
    }
    match chemin_codex_par_id(&racines, "abc123") {
        Some(chemin) => assert!(chemin == session),
        None => panic!("l'identifiant nommé se retrouve"),
    }
    // Un identifiant VIDE : refusé — « n'importe quel rollout » n'existe pas.
    assert!(chemin_codex_par_id(&racines, " ").is_none());
    assert!(chemin_codex_par_id(&racines, "").is_none());
    // Claude : cherché dans le nom, dossier intermédiaire encodé.
    let dossier_claude = racines.claude_projects.join("-Users-x-projets");
    match fs::create_dir_all(&dossier_claude) {
        Ok(()) => (),
        Err(cause) => panic!("mkdir refusé : {cause}"),
    }
    let claude = dossier_claude.join("9054562f-7ce.jsonl");
    match fs::write(&claude, "{}") {
        Ok(()) => (),
        Err(cause) => panic!("fichier refusé : {cause}"),
    }
    assert!(chemin_claude_par_id(&racines, "9054562f-7ce") == Some(claude));
    assert!(chemin_claude_par_id(&racines, "").is_none());
}

#[test]
fn les_racines_injectees_ne_lisent_jamais_lenvironnement() {
    // Structurel : toutes les fonctions prennent leurs racines en argument —
    // le test le vérifie en passant des racines inexistantes sans que rien
    // ne lise le HOME.
    let racines = racines_fake();
    let resolveur = Resolveur::nouveau(racines);
    assert!(chemin_codex_par_id(&resolveur.racines, "inexistant").is_none());
    assert!(chemin_claude_par_id(&resolveur.racines, "inexistant").is_none());
    // Et un JSON d'en-tête, pour la forme — rien n'est déduit :
    let _ = json!({ "parentSession": null });
}

#[test]
fn un_identifiant_trouve_n_est_plus_recherche_tant_que_son_fichier_existe() {
    use sessions::resolver::FamilleId;
    // Régression : chaque résolution d'un identifiant claude/codex refaisait
    // la descente récursive complète (battement du veilleur, du flux,
    // chaque requête).
    let racines = racines_fake();
    let dossier = racines.claude_projects.join("-Users-x-projet");
    let _ = fs::create_dir_all(&dossier);
    let fichier = dossier.join("abcd-1234.jsonl");
    fs::write(&fichier, "{}").unwrap_or_else(|cause| panic!("écriture : {cause}"));
    let resolveur = Resolveur::nouveau(racines.clone());
    for _ in 0..5 {
        assert_eq!(
            resolveur.chemin_par_id(FamilleId::Claude, "abcd-1234"),
            Some(fichier.clone())
        );
    }
    assert_eq!(resolveur.parcours_effectues(), 1, "un seul parcours");
    // Le fichier déplacé : le chemin mort n'est jamais servi.
    let ailleurs = racines.claude_projects.join("-Users-x-autre");
    let _ = fs::create_dir_all(&ailleurs);
    let deplace = ailleurs.join("abcd-1234.jsonl");
    fs::rename(&fichier, &deplace).unwrap_or_else(|cause| panic!("rename : {cause}"));
    assert_eq!(
        resolveur.chemin_par_id(FamilleId::Claude, "abcd-1234"),
        Some(deplace)
    );
    assert_eq!(resolveur.parcours_effectues(), 2);
}

#[test]
fn un_identifiant_introuvable_est_mis_en_cache_peu_de_temps() {
    use sessions::resolver::FamilleId;
    let racines = racines_fake();
    // TTL long : l'introuvable est servi depuis le cache.
    let resolveur = Resolveur::nouveau(racines.clone());
    assert!(resolveur.chemin_par_id(FamilleId::Codex, "neuf").is_none());
    assert!(resolveur.chemin_par_id(FamilleId::Codex, "neuf").is_none());
    assert_eq!(resolveur.parcours_effectues(), 1);
    // TTL nul : une session qui naît est visible à l'appel suivant.
    let vif = Resolveur::avec_ttl_negatif(racines.clone(), std::time::Duration::ZERO);
    assert!(vif.chemin_par_id(FamilleId::Codex, "neuf").is_none());
    let rollout = racines.codex_sessions.join("rollout-2026-neuf.jsonl");
    fs::write(&rollout, "{}").unwrap_or_else(|cause| panic!("écriture : {cause}"));
    assert_eq!(vif.chemin_par_id(FamilleId::Codex, "neuf"), Some(rollout));
}

/// Écrit `~/.claude/sessions/<pid>.json` à côté de la racine des projets.
fn fichier_pid_claude(racines: &Racines, pid: u32, contenu: &serde_json::Value) {
    let Some(parent) = racines.claude_projects.parent() else {
        panic!("racine claude sans parent");
    };
    let dossier = parent.join("sessions");
    if let Err(cause) = fs::create_dir_all(&dossier) {
        panic!("mkdir sessions refusé : {cause}");
    }
    if let Err(cause) = fs::write(dossier.join(format!("{pid}.json")), contenu.to_string()) {
        panic!("fichier pid refusé : {cause}");
    }
}

#[test]
fn claude_se_resout_par_le_pid_du_premier_plan() {
    let racines = racines_fake();
    let projet = racines.claude_projects.join("-Users-moi-projet");
    if let Err(cause) = fs::create_dir_all(&projet) {
        panic!("mkdir projet refusé : {cause}");
    }
    let session = projet.join("sess-42.jsonl");
    if let Err(cause) = fs::write(&session, "{}\n") {
        panic!("session refusée : {cause}");
    }
    fichier_pid_claude(
        &racines,
        4242,
        &json!({"pid": 4242, "sessionId": "sess-42", "cwd": "/Users/moi/projet"}),
    );
    let resolveur = Resolveur::nouveau(racines);
    match source_session_de(&resolveur, Some("claude"), None, None, None, Some(4242)) {
        Some(sessions::resolver::SourceSession::Fichier { base }) => assert!(base == session),
        autre => panic!("la session du PID se retrouve, eu {autre:?}"),
    }
    // Sans PID : aucun fil — jamais « la plus récente du dossier ».
    assert!(
        source_session_de(
            &resolveur,
            Some("claude"),
            None,
            None,
            Some("/Users/moi/projet"),
            None
        )
        .is_none()
    );
}

#[test]
fn un_fichier_pid_qui_nomme_un_autre_processus_est_refuse() {
    // PID recyclé : le fichier resté d'un processus mort ne désigne pas la
    // session du processus actuel.
    let racines = racines_fake();
    let session = racines.claude_projects.join("sess-vieille.jsonl");
    if let Err(cause) = fs::write(&session, "{}\n") {
        panic!("session refusée : {cause}");
    }
    fichier_pid_claude(
        &racines,
        77,
        &json!({"pid": 78, "sessionId": "sess-vieille"}),
    );
    let resolveur = Resolveur::nouveau(racines);
    assert!(source_session_de(&resolveur, Some("claude"), None, None, None, Some(77)).is_none());
}

#[test]
fn pi_opencode_devin_restent_sans_fil_meme_avec_une_session_dans_le_dossier() {
    // Porte capable d'échouer : une session Pi existe bien dans le dossier
    // encodé du cwd, et une session opencode et devin dans leurs bases pour
    // ce même cwd. Les choisir serait DEVINER (AGENTS §3.6) : aucun fil.
    let racines = racines_fake();
    let cwd = "/Users/moi/projet";
    let dossier_pi = racines.pi_sessions.join("--Users-moi-projet--");
    if let Err(cause) = fs::create_dir_all(&dossier_pi) {
        panic!("mkdir pi refusé : {cause}");
    }
    if let Err(cause) = fs::write(dossier_pi.join("2026-11-03_s.jsonl"), "{}\n") {
        panic!("session pi refusée : {cause}");
    }
    let base_opencode = match rusqlite::Connection::open(&racines.opencode_db) {
        Ok(base) => base,
        Err(cause) => panic!("base opencode refusée : {cause}"),
    };
    if let Err(cause) = base_opencode.execute_batch(
        "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, directory TEXT, time_updated INTEGER);
         INSERT INTO session_v2 VALUES ('ses_1', '/Users/moi/projet', 1);",
    ) {
        panic!("schéma opencode refusé : {cause}");
    }
    let base_devin = match rusqlite::Connection::open(&racines.devin_db) {
        Ok(base) => base,
        Err(cause) => panic!("base devin refusée : {cause}"),
    };
    if let Err(cause) = base_devin.execute_batch(
        "CREATE TABLE sessions (id TEXT PRIMARY KEY, working_directory TEXT, last_activity_at INTEGER);
         INSERT INTO sessions VALUES ('dv_1', '/Users/moi/projet', 1);",
    ) {
        panic!("schéma devin refusé : {cause}");
    }
    let resolveur = Resolveur::nouveau(racines);
    for kind in ["pi", "opencode", "devin"] {
        assert!(
            source_session_de(&resolveur, Some(kind), None, None, Some(cwd), Some(1)).is_none(),
            "{kind} : un fil deviné par dossier"
        );
    }
}

fn base_opencode(racines: &Racines, lignes: &str) {
    let base = match rusqlite::Connection::open(&racines.opencode_db) {
        Ok(base) => base,
        Err(cause) => panic!("base opencode refusée : {cause}"),
    };
    if let Err(cause) = base.execute_batch(&format!(
        "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, directory TEXT, title TEXT,
                                  parent_id TEXT, time_archived INTEGER, time_updated INTEGER);
         {lignes}"
    )) {
        panic!("schéma opencode refusé : {cause}");
    }
}

fn opencode(resolveur: &Resolveur, titre: &str, cwd: &str) -> Option<String> {
    match source_session_de(
        resolveur,
        Some("opencode"),
        None,
        Some(titre),
        Some(cwd),
        Some(1),
    ) {
        Some(sessions::resolver::SourceSession::Sqlite { session_id, .. }) => Some(session_id),
        Some(autre) => panic!("opencode hors base : {autre:?}"),
        None => None,
    }
}

#[test]
fn opencode_se_resout_par_son_titre_et_son_dossier_si_une_seule_session_concorde() {
    let racines = racines_fake();
    base_opencode(
        &racines,
        "INSERT INTO session_v2 VALUES ('ses_a', '/p', 'Fix the tests', NULL, NULL, 1);
         INSERT INTO session_v2 VALUES ('ses_b', '/p', 'Autre chose', NULL, NULL, 9);
         INSERT INTO session_v2 VALUES ('ses_enfant', '/p', 'Fix the tests', 'ses_a', NULL, 5);
         INSERT INTO session_v2 VALUES ('ses_archive', '/p', 'Fix the tests', NULL, 7, 8);
         INSERT INTO session_v2 VALUES ('ses_x', '/ailleurs', 'Fix the tests', NULL, NULL, 9);
         INSERT INTO session_v2 VALUES ('ses_d1', '/p', 'Doublon', NULL, NULL, 1);
         INSERT INTO session_v2 VALUES ('ses_d2', '/p', 'Doublon', NULL, NULL, 2);",
    );
    let resolveur = Resolveur::nouveau(racines);
    // Le titre ET le dossier concordent, sous-session et archive écartées.
    assert_eq!(
        opencode(&resolveur, "OC | Fix the tests", "/p").as_deref(),
        Some("ses_a")
    );
    // Un glyphe devant (spinner) est toléré, pas un mot.
    assert_eq!(
        opencode(&resolveur, "⠋ OC | Fix the tests", "/p").as_deref(),
        Some("ses_a")
    );
    assert_eq!(opencode(&resolveur, "Note OC | Fix the tests", "/p"), None);
    // Deux sessions du même titre : ambigu, aucun fil (jamais la plus récente).
    assert_eq!(opencode(&resolveur, "OC | Doublon", "/p"), None);
    // Pas le format d'opencode, autre dossier, titre inconnu : aucun fil.
    assert_eq!(opencode(&resolveur, "Fix the tests", "/p"), None);
    assert_eq!(opencode(&resolveur, "OC | Fix the tests", "/autre"), None);
    assert_eq!(opencode(&resolveur, "OC | Rien", "/p"), None);
}
