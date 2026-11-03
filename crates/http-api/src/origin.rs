//! La garde des requêtes mutantes (T049, S02) : origine, Content-Type,
//! taille déclarée — **avant toute action** (CURRENT.md §2).
//!
//! ⚠️ Le service n'a aucune authentification applicative : il écoute sur le
//! loopback et Tailscale Serve l'expose au tailnet. Tout appareil du tailnet
//! peut appeler `/api/agents/:paneId/prompt` — c'est le modèle décidé — mais
//! cela rend le **CSRF** propriété du navigateur. Ce garde ne remplace pas
//! une authentification : il ferme ce qui peut l'être sans elle — une
//! requête cross-site n'envoie pas d'`Origin` utilisable pour
//! `application/json`, et un formulaire HTML ne peut pas produire un corps
//! JSON. Les deux sont vérifiés ici.

use contracts::limits::MAX_BODY_BYTES;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

/// Redécouvre les origines publiques (Tailscale Serve) — bloquant, borné par
/// son propre délai.
pub type Redecouverte = Arc<dyn Fn() -> Vec<String> + Send + Sync>;

/// L'intervalle minimal entre deux redécouvertes : une rafale de requêtes
/// refusées ne lance pas une rafale de CLI.
pub const INTERVALLE_REDECOUVERTE: Duration = Duration::from_secs(10);

/// Les origines autorisées : celles de la configuration, plus celles qu'une
/// **redécouverte** a trouvées depuis.
///
/// ⚠️ **Retour du 2026-11-03 : « pour la énième fois, la PWA se voit rejeter
/// à cause de l'origin ».** Les origines Tailscale n'étaient lues qu'UNE fois,
/// au démarrage. Le 2026-11-02 à 20:18, juste après l'ouverture de session,
/// `tailscale serve status` a dépassé son délai : le démon a vécu sans
/// l'origine du tailnet, et chaque envoi de la PWA a pris un 403 jusqu'au
/// redémarrage suivant. Une origine inconnue relance donc la découverte avant
/// d'être refusée — une requête qui arrive par Serve prouve que Tailscale
/// répond. La règle de sécurité ne change pas : comparaison exacte, rien
/// n'est accepté sur un motif.
#[derive(Clone)]
pub struct OriginesAutorisees {
    interne: Arc<Interne>,
}

struct Interne {
    fixes: Vec<String>,
    decouvertes: RwLock<Vec<String>>,
    redecouverte: Option<Redecouverte>,
    intervalle: Duration,
    derniere: Mutex<Option<Instant>>,
}

impl OriginesAutorisees {
    /// Une liste figée — sans redécouverte.
    #[must_use]
    pub fn fixes(origines: Vec<String>) -> Self {
        Self::construit(origines, None, INTERVALLE_REDECOUVERTE)
    }

    /// Une liste qui se complète par `redecouverte`, au plus une fois par
    /// `intervalle`.
    #[must_use]
    pub fn avec_redecouverte(
        origines: Vec<String>,
        redecouverte: Redecouverte,
        intervalle: Duration,
    ) -> Self {
        Self::construit(origines, Some(redecouverte), intervalle)
    }

    fn construit(
        fixes: Vec<String>,
        redecouverte: Option<Redecouverte>,
        intervalle: Duration,
    ) -> Self {
        Self {
            interne: Arc::new(Interne {
                fixes,
                decouvertes: RwLock::new(Vec::new()),
                redecouverte,
                intervalle,
                derniere: Mutex::new(None),
            }),
        }
    }

    /// Les origines exactes autorisées maintenant (hors loopback).
    #[must_use]
    pub fn liste(&self) -> Vec<String> {
        let mut liste = self.interne.fixes.clone();
        let decouvertes = self
            .interne
            .decouvertes
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        for origine in decouvertes.iter() {
            if !liste.contains(origine) {
                liste.push(origine.clone());
            }
        }
        liste
    }

    /// Relance la découverte si elle existe et que la précédente date d'au
    /// moins `intervalle` — **bloquant**. Vrai quand la liste a changé.
    pub fn rafraichit(&self) -> bool {
        let Some(redecouverte) = &self.interne.redecouverte else {
            return false;
        };
        {
            let mut derniere = self
                .interne
                .derniere
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if derniere.is_some_and(|instant| instant.elapsed() < self.interne.intervalle) {
                return false;
            }
            *derniere = Some(Instant::now());
        }
        let trouvees = redecouverte();
        // Une découverte vide (Tailscale muet) n'efface pas ce qu'une
        // précédente avait trouvé.
        if trouvees.is_empty() {
            return false;
        }
        let mut decouvertes = self
            .interne
            .decouvertes
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if *decouvertes == trouvees {
            return false;
        }
        *decouvertes = trouvees;
        true
    }
}

impl From<Vec<String>> for OriginesAutorisees {
    fn from(origines: Vec<String>) -> Self {
        Self::fixes(origines)
    }
}

/// Les causes de refus — chacune avec son code HTTP propre.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RaisonGarde {
    OrigineAbsente,
    OrigineEtrangere,
    Methode,
    ContentType,
    CorpsTropGrand,
}

/// Un refus de garde — la cause, et pour l'origine les deux côtés.
#[derive(Clone, Debug)]
pub struct RefusGarde {
    pub raison: RaisonGarde,
    pub statut: u16,
    /// Les origines attendues (la configuration), pour un refus d'origine.
    pub attendue: Option<String>,
    /// L'origine reçue, pour un refus d'origine.
    pub recue: Option<String>,
}

/// Origine servie par la machine elle-même (loopback).
///
/// ⚠️ `localhost`, `[IP_ADDRESS]` et `[::1]` **tous ports confondus** : la
/// PWA est servie aussi bien par le backend que par le serveur de dev, et le
/// port change sans que la sécurité change. Une page servie par la machine ne
/// peut pas être un site tiers : l'accepter ne rouvre pas le CSRF, qui vient
/// précisément d'une origine **distante**.
/// L'après-schéma (`http://` / `https://`, casse libre), ou rien.
fn apres_schema(origine: &str) -> Option<&str> {
    const SCHEMAS: [&str; 2] = ["http://", "https://"];
    for schema in SCHEMAS {
        if origine.len() >= schema.len() && origine[..schema.len()].eq_ignore_ascii_case(schema) {
            return Some(&origine[schema.len()..]);
        }
    }
    None
}

#[must_use]
pub fn est_loopback(origine: &str) -> bool {
    let Some(hote) = hote_de(origine) else {
        return false;
    };
    hote == "localhost" || hote == "127.0.0.1" || hote == "::1"
}

/// L'hôte d'une origine, en minuscules, sans crochets — l'équivalent de
/// `new URL(origin).hostname.toLowerCase()` du fil. Une origine illisible
/// (pas de schéma, IPv6 brute) est `None` : écartée, pas devinée.
fn hote_de(origine: &str) -> Option<String> {
    // Le schéma est insensible à la casse (le fil s'en remet à `new URL`).
    let sans_schema = apres_schema(origine)?;
    let autorite = sans_schema.split('/').next()?;
    let hote = if let Some(apres) = autorite.strip_prefix('[') {
        let fin = apres.find(']')?;
        &apres[..fin]
    } else {
        match autorite.rfind(':') {
            Some(deux_points) => {
                let hote = &autorite[..deux_points];
                // Une adresse IPv6 BRUTE (sans crochets) est malformée.
                if hote.contains(':') {
                    return None;
                }
                hote
            }
            None => autorite,
        }
    };
    Some(hote.to_lowercase())
}

/// Origines autorisées : la liste exacte (config + Tailscale) **ou**
/// loopback. `https://macbook.ts.net.evil.com` ne doit pas passer pour
/// `https://macbook.ts.net` : pas de comparaison partielle, pas de motif.
#[must_use]
pub fn est_autorisee(origine: Option<&str>, attendues: &[String]) -> bool {
    match origine {
        Some(origine) => {
            attendues.iter().any(|attendue| attendue == origine) || est_loopback(origine)
        }
        None => false,
    }
}

/// Vérifie une requête mutante, dans l'ordre du fil : origine, Content-Type,
/// taille déclarée. Aucune action avant validation.
pub fn verifie_garde(
    origine: Option<&str>,
    content_type: Option<&str>,
    content_length: Option<&str>,
    attendues: &[String],
) -> Result<(), RefusGarde> {
    // 1. Origine. Une requête simple (formulaire, `<img>`, `<script>`) n'en
    //    envoie pas : c'est précisément le vecteur à fermer.
    let Some(origine) = origine.filter(|origine| !origine.is_empty()) else {
        return Err(RefusGarde {
            raison: RaisonGarde::OrigineAbsente,
            statut: 403,
            attendue: None,
            recue: None,
        });
    };
    if !est_autorisee(Some(origine), attendues) {
        // Les deux origines sont rapportées : celles attendues viennent de la
        // configuration, celle reçue vient du navigateur. Les nommer suffit à
        // corriger la configuration sans avoir à reproduire la requête.
        return Err(RefusGarde {
            raison: RaisonGarde::OrigineEtrangere,
            statut: 403,
            attendue: Some(attendues.join(" | ")),
            recue: Some(String::from(origine)),
        });
    }

    // 2. Type de contenu. Un formulaire HTML ne peut produire que
    //    `application/x-www-form-urlencoded`, `multipart/form-data` ou
    //    `text/plain` : les exiger rend le cross-site impossible.
    let content_type = content_type.unwrap_or_default();
    if !content_type
        .to_ascii_lowercase()
        .starts_with("application/json")
    {
        return Err(RefusGarde {
            raison: RaisonGarde::ContentType,
            statut: 415,
            attendue: None,
            recue: None,
        });
    }

    // 3. Taille déclarée. Un corps énorme est refusé **avant** d'être lu : le
    //    lire pour le refuser ensuite consommerait la mémoire pour rien. La
    //    taille RÉELLE (même chunked) est tranchée à la lecture (C09).
    if let Ok(longueur) = content_length.unwrap_or_default().trim().parse::<usize>()
        && longueur > MAX_BODY_BYTES
    {
        return Err(RefusGarde {
            raison: RaisonGarde::CorpsTropGrand,
            statut: 413,
            attendue: None,
            recue: None,
        });
    }

    Ok(())
}

/// Message : la cause, jamais un refus muet.
#[must_use]
pub fn describe_refus(raison: RaisonGarde, attendue: Option<&str>, recue: Option<&str>) -> String {
    match raison {
        RaisonGarde::OrigineAbsente => String::from("missing origin: likely cross-site request"),
        RaisonGarde::OrigineEtrangere => format!(
            "unauthorized origin (expected {}, received {})",
            attendue.unwrap_or("?"),
            recue.unwrap_or("?")
        ),
        RaisonGarde::Methode => String::from("method not allowed on this route"),
        RaisonGarde::ContentType => String::from("expected Content-Type: application/json"),
        RaisonGarde::CorpsTropGrand => {
            format!("body too large (maximum {MAX_BODY_BYTES} bytes)")
        }
    }
}
