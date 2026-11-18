//! Le résolveur partagé (T023/C15) — **le seul point de décision** : overview,
//! liste et session passent tous par ici (deux résolutions parallèles avaient
//! déjà produit deux réponses différentes — D013).
//!
//! ⚠️ **Les racines sont injectées** (config de démarrage) : aucune fonction
//! de ce module ne lit l'environnement (TECH §6) — les tests n'écrivent jamais
//! dans les vrais dossiers de la machine.
//!
//! ⚠️ **Les bases d'index codex portent une version NUMÉRIQUE**
//! (`state_5.sqlite`) : coder le numéro ferait taire l'index le jour où codex
//! le change ; le tri est **numérique et décroissant** — `state_10` bat
//! `state_9` (le tri lexicographique du fil inversait les deux : la régression
//! d19f98d, corrigée ici).
//!
//! ⚠️ **Le cache positif/négatif s'invalide par la base elle-même** : la
//! validité d'une entrée (trouvée OU pas trouvée) est le couple
//! (mtime, taille) de chaque base d'index ET de son `-wal` — l'ajout d'une
//! session change l'un des deux, donc un cache négatif s'invalide et la
//! nouvelle session est **visible**.

use crate::storage::sqlite::worker::Ouvrier;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Où vit la session d'un agent — ⚠️ **deux magasins, pas deux capacités** :
/// pi/codex/claude écrivent un FICHIER ; opencode/devin écrivent une base
/// SQLite et désignent une session par sa **clé primaire**. La distinction est
/// une question de forme — elle décide COMMENT lire, jamais SI on peut lire.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SourceSession {
    Fichier {
        base: PathBuf,
    },
    Sqlite {
        base: PathBuf,
        session_id: String,
        magasin: Magasin,
    },
}

/// Quel magasin SQLite — deux agents, deux bases, deux schémas, une seule
/// forme de page en sortie.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Magasin {
    Opencode,
    Devin,
}

/// Le référentiel dans lequel Herdr désigne la session (`kind: "path"|"id"`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Emplacement {
    pub kind: String,
    pub value: String,
}

impl Emplacement {
    #[must_use]
    pub fn nouveau(kind: &str, value: &str) -> Self {
        Self {
            kind: kind.to_string(),
            value: value.to_string(),
        }
    }
}

/// Les racines injectées du résolveur.
#[derive(Clone, Debug)]
pub struct Racines {
    /// `PI_SESSIONS_ROOT` — Herdr publie le chemin des sessions Pi ; la
    /// racine sert au garde de lecture (S03), jamais à chercher.
    pub pi_sessions: PathBuf,
    /// `CODEX_SESSION_ROOT` — les fichiers `rollout-*-{id}.jsonl`.
    pub codex_sessions: PathBuf,
    /// `CODEX_HOME` — les bases d'index `state_*.sqlite`.
    pub codex_home: PathBuf,
    /// `CLAUDE_PROJECTS_ROOT` — les fichiers `{id}.jsonl`.
    pub claude_projects: PathBuf,
    /// `OPENCODE_DB`.
    pub opencode_db: PathBuf,
    /// `DEVIN_DB`.
    pub devin_db: PathBuf,
}

struct EntreeCache {
    chemin: Option<PathBuf>,
    validite: Vec<(SystemTime, u64)>,
}

/// La validité du cache d'index codex : (mtime, taille) de **chaque base**
/// `state_N.sqlite` **et de son journal `-wal`**.
///
/// ⚠️ Elle lisait les métadonnées du **dossier** `CODEX_HOME` : l'insertion
/// d'une ligne dans une base ne change pas le mtime du dossier, et un fil pas
/// encore indexé restait « introuvable » en cache jusqu'à ce qu'un fichier
/// soit créé ou supprimé à côté. En mode WAL, la base principale ne bouge
/// qu'au checkpoint : c'est le `-wal` qui grandit.
fn validite_index_codex(codex_home: &Path) -> Vec<(SystemTime, u64)> {
    let mesure = |chemin: &Path| match fs::metadata(chemin) {
        Ok(meta) => (
            meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            meta.len(),
        ),
        Err(_) => (SystemTime::UNIX_EPOCH, 0),
    };
    bases_codex(codex_home)
        .iter()
        .flat_map(|base| {
            let mut wal = base.clone().into_os_string();
            wal.push("-wal");
            [mesure(base), mesure(Path::new(&wal))]
        })
        .collect()
}

/// Le résolveur — cache positif/négatif invalidable en mémoire.
pub struct Resolveur {
    pub racines: Racines,
    cache: Mutex<HashMap<CleIndex, EntreeCache>>,
    /// Le cache identifiant → fichier (codex, claude) : voir
    /// [`Resolveur::chemin_par_id`].
    ids: Mutex<HashMap<(FamilleId, String), EntreeId>>,
    /// La durée de vie d'un « introuvable » en cache.
    ttl_negatif: Duration,
    /// Combien de parcours de dossiers ont réellement eu lieu — l'observation
    /// des tests (un cache qui ne sert jamais est un ornement).
    parcours: AtomicU64,
}

/// Les deux familles dont l'identifiant se cherche dans un NOM de fichier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FamilleId {
    Codex,
    Claude,
}

struct EntreeId {
    chemin: Option<PathBuf>,
    mesure: Instant,
}

/// La durée de vie d'un « introuvable » : 5 s. Une session claude/codex qui
/// naît après une recherche infructueuse devient visible au plus tard après
/// ce délai — deux battements du veilleur.
pub const TTL_NEGATIF_IDS: Duration = Duration::from_secs(5);

/// La borne du cache d'identifiants : au-delà, il repart vide (jamais une
/// croissance sans fin sur un démon qui tourne des semaines).
const CAPACITE_IDS: usize = 1024;

/// Les bases d'index codex, de la plus récente à la plus ancienne — tri
/// **numérique** de la version (la régression d19f98d : le tri lexicographique
/// plaçait `state_9` devant `state_10`).
#[must_use]
pub fn bases_codex(dossier_codex: &Path) -> Vec<PathBuf> {
    let entrees = match fs::read_dir(dossier_codex) {
        Ok(entrees) => entrees,
        // Une base absente ne rend simplement rien.
        Err(_) => return Vec::new(),
    };
    let mut versionnees: Vec<(u64, PathBuf)> = entrees
        .filter_map(|entree| entree.ok())
        .filter_map(|entree| {
            let nom = entree.file_name().into_string().ok()?;
            if !nom.starts_with("state_") || !nom.ends_with(".sqlite") {
                return None;
            }
            let version: u64 = nom
                .strip_prefix("state_")?
                .strip_suffix(".sqlite")?
                .parse()
                .ok()?;
            Some((version, entree.path()))
        })
        .collect();
    versionnees.sort_by(|a, b| b.0.cmp(&a.0));
    versionnees.into_iter().map(|(_, chemin)| chemin).collect()
}

/// Le titre d'un terminal codex, lu : les noms candidats du fil et le
/// dossier que codex affiche.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TitreCodex {
    /// La lecture brute, puis (s'il y en a une) la lecture sans décoration.
    pub noms: Vec<String>,
    /// Le dossier affiché après ` | `, tel quel.
    pub dossier: String,
}

impl TitreCodex {
    /// Le titre nomme-t-il ce dossier ? Le **dernier segment** du chemin, en
    /// entier — ou son début quand codex l'a tronqué.
    ///
    /// ⚠️ **Mesuré le 2026-11-18 (codex 0.169)** : un nom de dossier long est
    /// coupé, suivi de `...` (`Improve 3MF ceiling curve |
    /// Grotte_105x90x90mm_ap...` pour `Grotte_105x90x90mm_appuis_plats_STL_3MF`).
    /// Comparé en entier, le dossier ne concordait plus : l'agent restait
    /// « sans session lisible ».
    #[must_use]
    pub fn nomme_le_dossier(&self, chemin: &str) -> bool {
        let segment = chemin
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("");
        if segment.is_empty() {
            return false;
        }
        if segment == self.dossier {
            return true;
        }
        ["...", "…"].iter().any(|coupure| {
            self.dossier
                .strip_suffix(coupure)
                .is_some_and(|debut| !debut.is_empty() && segment.starts_with(debut))
        })
    }
}

/// Lit un titre codex `<nom du fil> | <dossier>` — `None` s'il n'a pas
/// cette forme (pas de ` | `, nom ou dossier vide).
///
/// ⚠️ **mesuré** : le titre porte un **glyphe d'état quand codex travaille**
/// (`⠼ Audit PWA and webapp | herdr-pwa`). On ne « nettoie » pas au jugé :
/// on rend la lecture **brute** ET la lecture sans décoration, et c'est
/// l'index qui tranche — un nom qui commence vraiment par un glyphe reste
/// trouvable, et un faux nettoyage ne peut pas produire une session (il
/// produit un nom que l'index ignore).
#[must_use]
pub fn lis_titre_codex(titre: &str) -> Option<TitreCodex> {
    let barre = titre.rfind(" | ").filter(|barre| *barre > 0)?;
    let prefixe = titre[..barre].trim();
    let dossier = titre[barre + 3..].trim();
    if prefixe.is_empty() || dossier.is_empty() {
        return None;
    }
    Some(TitreCodex {
        noms: lectures_du_nom(prefixe),
        dossier: dossier.to_string(),
    })
}

/// Les noms de session codex, lus dans le titre du pane — seulement si le
/// titre nomme le dossier du pane (voir [`TitreCodex::nomme_le_dossier`]).
#[must_use]
pub fn noms_de_session_codex(titre: Option<&str>, cwd: Option<&str>) -> Vec<String> {
    match (titre.and_then(lis_titre_codex), cwd) {
        (Some(lu), Some(cwd)) if lu.nomme_le_dossier(cwd) => lu.noms,
        _ => Vec::new(),
    }
}

/// Les lectures du nom : brute, puis sans décoration quand elle diffère.
fn lectures_du_nom(prefixe: &str) -> Vec<String> {
    // La lecture sans décoration : les premiers caractères non lettre/non
    // chiffre + espace tombent (le glyphe du spinner).
    let sans_decoration: String = {
        let mut reste = prefixe;
        while let Some(caractere) = reste.chars().next() {
            if caractere.is_alphanumeric() {
                break;
            }
            reste = reste[caractere.len_utf8()..].trim_start();
            if reste.len() < prefixe.len() {
                break; // après TRIM, on attend l'espace qui séparait la décoration
            }
        }
        // La règle du fil : `^[^\p{L}\p{N}]+\s+` — une suite de non-lettres
        // NON suivie d'un espace ne compte pas comme décoration.
        match prefixe.find(' ') {
            Some(coupure)
                if !prefixe[..coupure]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric) =>
            {
                prefixe[coupure + 1..].trim().to_string()
            }
            _ => String::new(),
        }
    };
    if !sans_decoration.is_empty() && sans_decoration != prefixe {
        vec![prefixe.to_string(), sans_decoration]
    } else {
        vec![prefixe.to_string()]
    }
}

/// Le nom de session codex « simple » — le **premier** candidat : la lecture
/// brute, celle qui ne suppose rien.
#[must_use]
pub fn nom_de_session_codex(titre: Option<&str>, cwd: Option<&str>) -> Option<String> {
    noms_de_session_codex(titre, cwd).into_iter().next()
}

/// Parcours récursif borné — l'homologue des `globSync` du fil, sans
/// dépendance : les fichiers dont le NOM passe la garde, triés.
fn parcours(racine: &Path, garde: &impl Fn(&str) -> bool, sortie: &mut Vec<PathBuf>) {
    let entrees = match fs::read_dir(racine) {
        Ok(entrees) => entrees,
        Err(_) => return,
    };
    for entree in entrees.filter_map(|entree| entree.ok()) {
        let chemin = entree.path();
        if chemin.is_dir() {
            parcours(&chemin, garde, sortie);
        } else if garde(&entree.file_name().into_string().unwrap_or_default()) {
            sortie.push(chemin);
        }
    }
}

/// Résout un identifiant codex en chemin de fichier — l'identifiant est
/// cherché dans le **NOM** du fichier (la disposition de codex), motif ancré
/// sur `rollout-` et le séparateur qui précède : un identifiant ne peut pas
/// se faire passer pour le préfixe d'un autre. Un identifiant vide rendrait
/// « n'importe quel rollout » : refusé.
#[must_use]
pub fn chemin_codex_par_id(racines: &Racines, id: &str) -> Option<PathBuf> {
    if id.trim().is_empty() {
        return None;
    }
    let suffixe = format!("-{id}.jsonl");
    let mut trouves: Vec<PathBuf> = Vec::new();
    let garde = |nom: &str| nom.starts_with("rollout-") && nom.ends_with(&suffixe);
    parcours(&racines.codex_sessions, &garde, &mut trouves);
    trouves.sort();
    trouves.into_iter().next()
}

/// L'identifiant de session Claude d'un processus (herdr-core ne publie aucun
/// lien vers la session) : `~/.claude/sessions/<pid>.json` → `sessionId`.
/// Le dossier est le voisin de `claude_projects` (`~/.claude/sessions`).
///
/// ⚠️ Le fichier doit **nommer ce même PID** (`pid`) quand il le porte : un
/// fichier resté d'un processus mort dont le PID a été recyclé ne désigne pas
/// la session du processus actuel.
#[must_use]
pub fn session_claude_par_pid(racines: &Racines, pid: u32) -> Option<String> {
    let dossier = racines.claude_projects.parent()?.join("sessions");
    let contenu = std::fs::read_to_string(dossier.join(format!("{pid}.json"))).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contenu).ok()?;
    if let Some(annonce) = json.get("pid").and_then(serde_json::Value::as_u64)
        && annonce != u64::from(pid)
    {
        return None;
    }
    let session_id = json.get("sessionId").and_then(serde_json::Value::as_str)?;
    if session_id.trim().is_empty() {
        return None;
    }
    Some(session_id.to_string())
}

/// Le titre de session qu'opencode publie dans le titre du terminal :
/// `OC | <titre>`, éventuellement précédé d'un glyphe (spinner).
#[must_use]
pub fn titre_de_session_opencode(titre_terminal: &str) -> Option<&str> {
    let debut = titre_terminal.find("OC | ")?;
    if titre_terminal[..debut].chars().any(char::is_alphanumeric) {
        return None;
    }
    let titre = titre_terminal[debut + 5..].trim();
    (!titre.is_empty()).then_some(titre)
}

/// L'identifiant de la session opencode que le terminal affiche : titre
/// EXACT et dossier EXACT, session principale (`parent_id` nul), non
/// archivée — et **une seule**. Deux sessions du même titre dans le même
/// dossier : aucun fil (ambigu), jamais la plus récente.
#[must_use]
pub fn session_opencode_par_titre(
    racines: &Racines,
    titre_terminal: &str,
    cwd: &str,
) -> Option<String> {
    let titre = titre_de_session_opencode(titre_terminal)?;
    if cwd.trim().is_empty() {
        return None;
    }
    let base = rusqlite::Connection::open_with_flags(
        &racines.opencode_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let mut requete = base
        .prepare(
            "SELECT id FROM session_v2 WHERE directory = ?1 AND title = ?2 \
             AND parent_id IS NULL AND time_archived IS NULL LIMIT 2",
        )
        .ok()?;
    let ids: Vec<String> = requete
        .query_map(rusqlite::params![cwd, titre], |ligne| ligne.get(0))
        .ok()?
        .filter_map(Result::ok)
        .collect();
    match ids.as_slice() {
        [id] if !id.trim().is_empty() => Some(id.clone()),
        _ => None,
    }
}

/// Résout un identifiant claude en chemin — cherché dans le nom du fichier
/// (le dossier intermédiaire est le chemin de travail encodé, qui ne se
/// déduit pas d'un identifiant : on cherche, on ne compose pas).
#[must_use]
pub fn chemin_claude_par_id(racines: &Racines, id: &str) -> Option<PathBuf> {
    if id.trim().is_empty() {
        return None;
    }
    let suffixe = format!("{id}.jsonl");
    let mut trouves: Vec<PathBuf> = Vec::new();
    let garde = |nom: &str| nom.ends_with(&suffixe);
    parcours(&racines.claude_projects, &garde, &mut trouves);
    trouves.sort();
    trouves.into_iter().next()
}

/// Chemin du fichier de session codex, retrouvé par le **nom du fil** et le
/// dossier, dans l'index de codex — ⚠️ **trois garde-fous rendent la preuve
/// vérifiable** : le nom ET le dossier doivent correspondre, et le fichier
/// doit exister. Sans les trois : `None` — jamais la session d'un autre.
/// ⚠️ Le cache porte les résultats TROUVÉS comme NON TROUVÉS, invalidés par
/// la base elle-même : une session ajoutée devient visible.
#[must_use]
pub fn chemin_codex_par_nom(resolveur: &Resolveur, nom: &str, cwd: &str) -> Option<PathBuf> {
    if nom.trim().is_empty() || cwd.is_empty() {
        return None;
    }
    let cle = CleIndex::Dossier {
        nom: nom.to_string(),
        cwd: cwd.to_string(),
    };
    cherche_dans_l_index(resolveur, cle, |ouvrier| {
        let lignes = ouvrier
            .interroge(
                "select rollout_path from threads where name = ?1 and cwd = ?2 and archived = 0 limit 2",
                &[nom, cwd],
            )
            .ok()?;
        /*
          ⚠️ **Deux fils du même nom dans le même dossier : aucun.** Prendre
          le plus récent (`order by updated_at desc limit 1`) affichait, pour
          un pane, la conversation d'un AUTRE pane lancé avec la même
          première consigne. Comme pour opencode, l'ambiguïté ne prouve rien.
        */
        match lignes.as_slice() {
            [ligne] => chemin_existant(ligne),
            _ => None,
        }
    })
}

/// Chemin du fichier de session codex, retrouvé par le **nom du fil** et le
/// **dossier du fil** tel que le titre le nomme — sans le dossier du pane.
///
/// ⚠️ **Mesuré le 2026-11-18** : `codex resume` lancé depuis
/// `~/projects/perso/dashboard` reprend un fil né dans `~` ; le titre dit
/// `Réduire l’épaisseur du tronc | thomas.delalonde` (le dossier du FIL),
/// l'index aussi (`cwd = /Users/thomas.delalonde`), le pane non. Exiger le
/// dossier du pane laissait l'agent « sans session lisible ».
///
/// Les garde-fous restent : le nom exact, un dossier de fil que le titre
/// nomme, le fichier existe — et **un seul** fil concorde. Deux fils du même
/// nom dont les dossiers portent le même nom : aucun.
#[must_use]
pub fn chemin_codex_par_dossier_du_fil(
    resolveur: &Resolveur,
    nom: &str,
    titre: &TitreCodex,
) -> Option<PathBuf> {
    if nom.trim().is_empty() {
        return None;
    }
    let cle = CleIndex::DossierDuFil {
        nom: nom.to_string(),
        dossier: titre.dossier.clone(),
    };
    cherche_dans_l_index(resolveur, cle, |ouvrier| {
        let lignes = ouvrier
            .interroge(
                "select rollout_path, cwd from threads where name = ?1 and archived = 0",
                &[nom],
            )
            .ok()?;
        let mut concordants = lignes.iter().filter(|ligne| {
            ligne
                .valeurs
                .get(1)
                .and_then(Value::as_str)
                .is_some_and(|cwd| titre.nomme_le_dossier(cwd))
        });
        let seul = concordants.next()?;
        if concordants.next().is_some() {
            return None;
        }
        chemin_existant(seul)
    })
}

/// Ce qu'on cherche dans l'index codex — la clé du cache.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum CleIndex {
    /// Le nom du fil et le dossier exact du pane.
    Dossier { nom: String, cwd: String },
    /// Le nom du fil et le dossier affiché par le titre.
    DossierDuFil { nom: String, dossier: String },
}

/// Le `rollout_path` d'une ligne d'index (première colonne), s'il existe.
fn chemin_existant(ligne: &crate::storage::sqlite::worker::Ligne) -> Option<PathBuf> {
    ligne
        .valeurs
        .first()
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|chemin| chemin.exists())
}

/// Interroge chaque base d'index (la plus récente d'abord) jusqu'à ce que
/// `cherche` trouve — résultat trouvé OU pas mis en cache, invalidé par les
/// bases elles-mêmes.
fn cherche_dans_l_index(
    resolveur: &Resolveur,
    cle: CleIndex,
    cherche: impl Fn(&Ouvrier) -> Option<PathBuf>,
) -> Option<PathBuf> {
    let validite = validite_index_codex(&resolveur.racines.codex_home);
    if let Ok(cache) = resolveur.cache.lock()
        && let Some(entree) = cache.get(&cle)
        && entree.validite == validite
    {
        return entree.chemin.clone();
    }
    // Une base illisible/absente/inattendue ne PROUVE rien : on essaie la
    // suivante, puis on renonce. Jamais de substitution.
    let chemin_trouve = bases_codex(&resolveur.racines.codex_home)
        .iter()
        .find_map(|base| cherche(&Ouvrier::nouveau(base).ok()?));
    if let Ok(mut cache) = resolveur.cache.lock() {
        cache.insert(
            cle,
            EntreeCache {
                chemin: chemin_trouve.clone(),
                validite,
            },
        );
    }
    chemin_trouve
}

/// Emplacement de session d'un pane codex, déduit de son **titre** — `None`
/// dès que le titre ne nomme pas un fil, ou que l'index ne le confirme pas :
/// le pane garde son panneau neutre, ce qui est exact. ⚠️ On essaie chaque
/// lecture contre l'index, et c'est LUI qui tranche — la première confirmée
/// gagne, aucune autre n'est inventée.
///
/// Deux preuves, dans cet ordre : (1) le titre nomme le dossier du pane et
/// l'index confirme nom + ce dossier exact ; (2) sinon (fil repris depuis un
/// autre dossier), l'index confirme le nom et **un seul** fil dont le
/// dossier est celui du titre.
#[must_use]
pub fn session_codex_par_titre(
    resolveur: &Resolveur,
    titre: Option<&str>,
    cwd: Option<&str>,
) -> Option<Emplacement> {
    let lu = lis_titre_codex(titre?)?;
    let vers = |chemin: PathBuf| Emplacement::nouveau("path", &chemin.to_string_lossy());
    if let Some(cwd) = cwd.filter(|cwd| !cwd.is_empty() && lu.nomme_le_dossier(cwd)) {
        for nom in &lu.noms {
            if let Some(chemin) = chemin_codex_par_nom(resolveur, nom, cwd) {
                return Some(vers(chemin));
            }
        }
    }
    lu.noms
        .iter()
        .find_map(|nom| chemin_codex_par_dossier_du_fil(resolveur, nom, &lu))
        .map(vers)
}

/// Source de la session d'un agent, ou `None` s'il n'y en a pas.
#[must_use]
pub fn source_session(
    resolveur: &Resolveur,
    agent_kind: Option<&str>,
    emplacement: Option<&Emplacement>,
) -> Option<SourceSession> {
    let emplacement = emplacement?;
    if emplacement.kind == "path" {
        return Some(SourceSession::Fichier {
            base: PathBuf::from(&emplacement.value),
        });
    }
    if emplacement.kind != "id" {
        return None;
    }
    match agent_kind {
        Some("codex") => {
            let chemin = resolveur.chemin_par_id(FamilleId::Codex, &emplacement.value)?;
            Some(SourceSession::Fichier { base: chemin })
        }
        Some("opencode") => {
            // Un identifiant vide désignerait n'importe quoi : refusé.
            if emplacement.value.trim().is_empty() {
                return None;
            }
            Some(SourceSession::Sqlite {
                base: resolveur.racines.opencode_db.clone(),
                session_id: emplacement.value.clone(),
                magasin: Magasin::Opencode,
            })
        }
        Some("claude") => {
            let chemin = resolveur.chemin_par_id(FamilleId::Claude, &emplacement.value)?;
            Some(SourceSession::Fichier { base: chemin })
        }
        Some("devin") => {
            if emplacement.value.trim().is_empty() {
                return None;
            }
            Some(SourceSession::Sqlite {
                base: resolveur.racines.devin_db.clone(),
                session_id: emplacement.value.clone(),
                magasin: Magasin::Devin,
            })
        }
        _ => None,
    }
}

/// L'emplacement de session d'un agent — ⚠️ **il empile deux questions, dans
/// cet ordre** : (1) Herdr a-t-il publié un emplacement ? Si oui, il fait
/// **foi**. (2) Sinon, et **seulement pour codex**, son titre nomme-t-il un
/// fil que son index confirme ? Le repli vit au même endroit que la
/// résolution ; les appelants ne font que le lire (D013).
#[must_use]
pub fn source_session_de(
    resolveur: &Resolveur,
    kind: Option<&str>,
    emplacement: Option<&Emplacement>,
    titre: Option<&str>,
    cwd: Option<&str>,
    pid: Option<u32>,
) -> Option<SourceSession> {
    let annonce = source_session(resolveur, kind, emplacement);
    if annonce.is_some() {
        return annonce;
    }
    // Claude : herdr-core ne publie aucun lien ; le PID du programme au
    // premier plan désigne sa session (`~/.claude/sessions/<pid>.json`,
    // EXÉCUTÉ). La recherche du fichier passe par le cache du résolveur.
    if kind == Some("claude") {
        let id = session_claude_par_pid(&resolveur.racines, pid?)?;
        return source_session(resolveur, kind, Some(&Emplacement::nouveau("id", &id)));
    }
    // opencode : son titre de terminal (`OC | <titre de session>`) ET son
    // dossier doivent désigner UNE seule session principale de sa base —
    // deux faits indépendants qui concordent (EXÉCUTÉ : le titre + dossier de
    // `opencode -s ses_…` rend exactement cet identifiant).
    if kind == Some("opencode") {
        let id = session_opencode_par_titre(&resolveur.racines, titre?, cwd?)?;
        return source_session(resolveur, kind, Some(&Emplacement::nouveau("id", &id)));
    }
    // Pi, devin : aucune preuve de lien entre un terminal et sa session sous
    // herdr-core (AGENTS §3.6). « La plus récente du dossier » serait un fil
    // DEVINÉ — deux agents du même dossier liraient le même : l'agent reste
    // « sans fil » tant qu'un lien n'est pas prouvé.
    if kind != Some("codex") {
        return None;
    }
    let repli = session_codex_par_titre(resolveur, titre, cwd)?;
    source_session(resolveur, kind, Some(&repli))
}

/// Le chemin de session d'un agent, ou `None` — ⚠️ un chemin n'existe que
/// pour un **fichier** : la base opencode n'est pas un chemin de session, et
/// la faire passer pour tel tromperait l'appelant.
#[must_use]
pub fn localise_session(
    resolveur: &Resolveur,
    kind: Option<&str>,
    emplacement: Option<&Emplacement>,
) -> Option<PathBuf> {
    match source_session(resolveur, kind, emplacement) {
        Some(SourceSession::Fichier { base }) => Some(base),
        _ => None,
    }
}

impl Resolveur {
    #[must_use]
    pub fn nouveau(racines: Racines) -> Self {
        Self::avec_ttl_negatif(racines, TTL_NEGATIF_IDS)
    }

    /// Le résolveur avec une durée de vie choisie pour les « introuvables »
    /// (les tests la mettent à zéro pour ne pas dormir).
    #[must_use]
    pub fn avec_ttl_negatif(racines: Racines, ttl_negatif: Duration) -> Self {
        Self {
            racines,
            cache: Mutex::new(HashMap::new()),
            ids: Mutex::new(HashMap::new()),
            ttl_negatif,
            parcours: AtomicU64::new(0),
        }
    }

    /// La durée de vie d'un « introuvable » en cache.
    #[must_use]
    pub fn ttl_negatif(&self) -> Duration {
        self.ttl_negatif
    }

    /// Combien de parcours de dossiers ont eu lieu depuis la création.
    #[must_use]
    pub fn parcours_effectues(&self) -> u64 {
        self.parcours.load(Ordering::Relaxed)
    }

    /// Le fichier d'un identifiant codex/claude — **mis en cache**.
    ///
    /// ⚠️ **Un parcours récursif à chaque appel** : chaque battement du
    /// veilleur (2 s), chaque battement de flux et chaque requête refaisaient
    /// la descente complète de `~/.codex/sessions` ou `~/.claude/projects`.
    /// Désormais :
    /// - **trouvé** : servi tant que le fichier **existe** encore (vérifié à
    ///   chaque appel — un fichier déplacé ou supprimé relance la recherche,
    ///   jamais un chemin mort servi) ;
    /// - **introuvable** : servi pendant [`TTL_NEGATIF_IDS`] seulement — une
    ///   session qui naît devient visible après ce délai.
    pub fn chemin_par_id(&self, magasin: FamilleId, id: &str) -> Option<PathBuf> {
        if id.trim().is_empty() {
            return None;
        }
        let cle = (magasin, id.to_string());
        if let Ok(ids) = self.ids.lock()
            && let Some(entree) = ids.get(&cle)
        {
            match &entree.chemin {
                Some(chemin) if chemin.is_file() => return Some(chemin.clone()),
                None if entree.mesure.elapsed() < self.ttl_negatif => return None,
                _ => {}
            }
        }
        self.parcours.fetch_add(1, Ordering::Relaxed);
        let chemin = match magasin {
            FamilleId::Codex => chemin_codex_par_id(&self.racines, id),
            FamilleId::Claude => chemin_claude_par_id(&self.racines, id),
        };
        if let Ok(mut ids) = self.ids.lock() {
            if ids.len() >= CAPACITE_IDS {
                ids.clear();
            }
            ids.insert(
                cle,
                EntreeId {
                    chemin: chemin.clone(),
                    mesure: Instant::now(),
                },
            );
        }
        chemin
    }
}
