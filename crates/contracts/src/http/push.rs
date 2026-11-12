//! Web Push : abonnement, désabonnement, clef VAPID en texte brut.

use crate::presence::Nullable;
use serde::{Deserialize, Serialize};

/// Le corps de désabonnement (inline mesuré — `{ unsubscribe: endpoint }`).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnsubscribeRequest {
    pub unsubscribe: String,
}

/// La présence d'un appareil (`PresenceRequest`, H23) : l'app est au premier
/// plan (`visible: true`) ou vient de la quitter.
///
/// ⚠️ **Tant qu'une PWA est au premier plan, AUCUNE notification ne part** —
/// quel que soit l'appareil où elle tourne, et qu'il ait activé les
/// notifications ou non (retour du 2026-11-12 : PWA ouverte sur la tablette,
/// le téléphone notifié quand même). Un appareil sans abonnement push n'a donc
/// pas d'endpoint : il s'identifie par `appareil`.
///
/// `appareil` (identifiant stable de la PWA) ou, pour une PWA déjà chargée qui
/// ne l'envoie pas, `endpoint` : l'un des deux est requis.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresenceRequest {
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub appareil: crate::presence::Field<String>,
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub endpoint: crate::presence::Field<String>,
    pub visible: bool,
}

/// Les deux clefs d'identité d'un abonnement (inline mesuré) — `auth` puis
/// `p256dh`, orthographe exacte du fil.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionKeys {
    pub auth: String,
    pub p256dh: String,
}

/// Le corps d'abonnement (inline mesuré) — `expirationTime` omissible ET
/// nullable (`Schema.optional(Schema.NullOr(Schema.Number))`).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRequest {
    pub endpoint: String,
    #[serde(
        default,
        deserialize_with = "crate::presence::optionnel_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    // Schéma : `anyOf [T, null]` UNE fois — l'Option de schemars aurait
    // dupliqué le null du Nullable.
    #[schemars(with = "crate::presence::Nullable<u64>")]
    pub expiration_time: Option<Nullable<u64>>,
    pub keys: SubscriptionKeys,
    /// Les genres de notification que cet appareil ne veut pas recevoir
    /// (réglages) — absent : tous.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub muted: Vec<NotificationKind>,
}

/// Un genre de notification, tel que les réglages le coupent : à toi
/// (`waiting`), fini (`done`), erreur (`error`), plus là (`gone`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NotificationKind {
    Waiting,
    Done,
    Error,
    Gone,
}

impl NotificationKind {
    /// Le mot du store et du payload (`notifications::payload::GENRES`).
    #[must_use]
    pub fn mot(self) -> &'static str {
        match self {
            NotificationKind::Waiting => "waiting",
            NotificationKind::Done => "done",
            NotificationKind::Error => "error",
            NotificationKind::Gone => "gone",
        }
    }
}

/// L'union **du corps de la route push** : le désabonnement est essayé
/// **d'abord** (précédence mesurée du fil — un corps `{unsubscribe}` n'est
/// jamais un abonnement), sinon l'abonnement.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum PushBody {
    /// Essayé en premier : `{ "unsubscribe": "..." }`.
    Unsubscribe(UnsubscribeRequest),
    /// Sinon : un abonnement complet.
    Subscribe(SubscribeRequest),
}

/// La règle du fil d'abonnement, mesurée dans la route : un endpoint doit
/// commencer par `https://` — sinon 400 (échantillon construit
/// `SubscribeHttpPrefixRule` du corpus).
#[must_use]
pub fn endpoint_en_https(endpoint: &str) -> bool {
    endpoint.starts_with("https://")
}

/// La clef VAPID publique, rendue **en texte brut** (`text/plain`) — un
/// résultat non JSON typé, pas un DTO JSON fictif (TECH §5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VapidKeyText(pub String);

/// L'acquittement vide : les deux routes push répondent **204 sans corps**
/// — un variant de résultat nommé, jamais un `{ok}` inventé.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AcquittementVide;
