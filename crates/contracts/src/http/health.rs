//! `GET /api/health` — l'état du daemon, tel qu'annoncé.

use crate::presence::Field;
use serde::{Deserialize, Serialize};

/// La santé du service (mesurée sur `HealthResponse` de api.ts).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub herdr: HerdrHealth,
    /// Absent ou valeur — un null explicite est un refus (`Schema.optional`).
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub log: Field<String>,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub uptime_ms: Field<u64>,
    /// La veille (statuts en direct, push) — absente quand le démon n'en a
    /// pas démarré.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub watcher: Field<WatcherHealth>,
}

/// L'état de la veille : vivante ou non, son dernier succès, sa dernière
/// panne — jamais tue.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WatcherHealth {
    /// La boucle tourne (fil démarré, pas arrêté).
    pub running: bool,
    /// La DERNIÈRE itération a échoué.
    pub failing: bool,
    /// Depuis combien de temps (ms) la dernière itération réussie.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub last_success_ago_ms: Field<u64>,
    /// Les itérations en échec depuis le démarrage.
    pub errors: u64,
    /// La dernière panne, telle quelle, et depuis combien de temps (ms).
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub last_error: Field<String>,
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub last_error_ago_ms: Field<u64>,
    /// Le flux d'événements de herdr-core est branché (lot 4) : la veille
    /// repasse dès qu'un fait change. `false` : cadence de 2 s seulement.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub events: Field<bool>,
    /// Depuis combien de temps (ms) le dernier signe du flux (événement ou
    /// battement de 15 s).
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub last_event_ago_ms: Field<u64>,
}

/// La partie Herdr de la santé — protocol/version **omissibles** (mesuré :
/// absents tant qu'aucun ping n'a réussi), socket et reachable toujours là.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HerdrHealth {
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub protocol: Field<u64>,
    pub reachable: bool,
    pub socket: String,
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub version: Field<String>,
    /// Pourquoi herdr-core ne répond pas (`unavailable`, `needs_update`,
    /// génération non prise en charge…) — absent quand il répond.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub error: Field<String>,
}
