use rusqlite::{Connection, OpenFlags, params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use crate::core::Event;

/// Database wrapper for SQLite storage
pub struct Db {
    conn: Connection,
}

/// Save data for database storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSave {
    pub id: String,
    pub name: String,
    pub scenario_id: String,
    pub tick: u32,
    pub year: i32,
    pub created_at: u64,
    pub world_state_json: String,
    pub player_state_json: String,
}

/// Dead actor data for database storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbDeadActor {
    pub id: String,
    pub tick_death: u32,
    pub year_death: i32,
    pub final_metrics_json: String,
    pub successor_ids_json: String,
}

impl Db {
    /// Open database connection with appropriate flags
    /// Creates the database file and directory if they don't exist
    pub fn open(path: &Path) -> Result<Self, String> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create database directory: {}", e))?;
        }

        // Open connection with flags for multi-threaded access
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;

        let conn = Connection::open_with_flags(path, flags)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        let db = Db { conn };

        // Enable WAL mode for better concurrent access
        db.conn
            .execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(|e| format!("Failed to enable WAL mode: {}", e))?;

        // Run schema migration
        db.migrate_schema()?;

        Ok(db)
    }

    /// Get database path using dirs::data_dir()
    pub fn default_path() -> Result<std::path::PathBuf, String> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| "Could not determine data directory".to_string())?;

        let db_dir = data_dir.join("engine13");
        Ok(db_dir.join("engine13.db"))
    }

    /// Initialize database schema (CREATE TABLE IF NOT EXISTS)
    fn migrate_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "
                -- Events table
                CREATE TABLE IF NOT EXISTS events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL UNIQUE,
                    tick INTEGER NOT NULL,
                    year INTEGER NOT NULL,
                    actor_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    description TEXT NOT NULL,
                    metrics_snapshot TEXT,
                    involved_actors TEXT,
                    tags TEXT,
                    is_key INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP
                );

                -- Add scenario_id column if not exists (migration for existing databases)
                ALTER TABLE events ADD COLUMN scenario_id TEXT;

                -- Indexes for events
                CREATE INDEX IF NOT EXISTS idx_events_actor ON events(actor_id);
                CREATE INDEX IF NOT EXISTS idx_events_tick ON events(tick);
                CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
                CREATE INDEX IF NOT EXISTS idx_events_key ON events(is_key);
                CREATE INDEX IF NOT EXISTS idx_events_scenario ON events(scenario_id);

                -- Saves table
                CREATE TABLE IF NOT EXISTS saves (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    scenario_id TEXT NOT NULL,
                    tick INTEGER NOT NULL,
                    year INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    world_state_json TEXT NOT NULL,
                    player_state_json TEXT NOT NULL,
                    saved_at TEXT DEFAULT CURRENT_TIMESTAMP
                );

                -- Index for saves
                CREATE INDEX IF NOT EXISTS idx_saves_scenario ON saves(scenario_id);

                -- Dead actors table
                CREATE TABLE IF NOT EXISTS dead_actors (
                    id TEXT PRIMARY KEY,
                    tick_death INTEGER NOT NULL,
                    year_death INTEGER NOT NULL,
                    final_metrics_json TEXT NOT NULL,
                    successor_ids_json TEXT NOT NULL,
                    died_at TEXT DEFAULT CURRENT_TIMESTAMP
                );
                ",
            )
            .map_err(|e| format!("Failed to migrate schema: {}", e))?;

        Ok(())
    }

    // ========================================================================
    // Event operations
    // ========================================================================

    /// Insert a single event
    pub fn insert_event(&self, event: &Event) -> Result<(), String> {
        let involved_actors = serde_json::to_string(&event.involved_actors)
            .unwrap_or_else(|_| "[]".to_string());
        let tags = serde_json::to_string(&event.tags)
            .unwrap_or_else(|_| "[]".to_string());
        let metrics_snapshot = serde_json::to_string(&event.metrics_snapshot)
            .unwrap_or_else(|_| "{}".to_string());

        self.conn
            .execute(
                "
                INSERT OR REPLACE INTO events
                (event_id, tick, year, actor_id, event_type, description, metrics_snapshot, involved_actors, tags, is_key, scenario_id)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ",
                params![
                    event.id,
                    event.tick,
                    event.year,
                    event.actor_id,
                    Self::event_type_to_string(&event.event_type),
                    event.description,
                    metrics_snapshot,
                    involved_actors,
                    tags,
                    if event.is_key { 1 } else { 0 },
                    event.scenario_id,
                ],
            )
            .map_err(|e| format!("Failed to insert event: {}", e))?;

        Ok(())
    }

    /// Delete all events for a scenario
    pub fn delete_events_for_scenario(&self, scenario_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM events WHERE scenario_id = ?1",
                params![scenario_id],
            )
            .map_err(|e| format!("Failed to delete events: {}", e))?;

        Ok(())
    }

    /// Insert multiple events in a batch (more efficient)
    pub fn insert_events_batch(&mut self, events: &[Event]) -> Result<(), String> {
        let tx = self.conn
            .transaction()
            .map_err(|e| format!("Failed to start transaction: {}", e))?;

        for event in events {
            let involved_actors = serde_json::to_string(&event.involved_actors)
                .unwrap_or_else(|_| "[]".to_string());
            let tags = serde_json::to_string(&event.tags)
                .unwrap_or_else(|_| "[]".to_string());
            let metrics_snapshot = serde_json::to_string(&event.metrics_snapshot)
                .unwrap_or_else(|_| "{}".to_string());

            tx.execute(
                "
                INSERT OR REPLACE INTO events 
                (event_id, tick, year, actor_id, event_type, description, metrics_snapshot, involved_actors, tags, is_key)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    event.id,
                    event.tick,
                    event.year,
                    event.actor_id,
                    Self::event_type_to_string(&event.event_type),
                    event.description,
                    metrics_snapshot,
                    involved_actors,
                    tags,
                    if event.is_key { 1 } else { 0 }
                ],
            )
            .map_err(|e| format!("Failed to insert event: {}", e))?;
        }

        tx.commit()
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(())
    }

    /// Get all events for a specific actor
    pub fn get_events_by_actor(&self, actor_id: &str) -> Result<Vec<Event>, String> {
        let mut stmt = self.conn
            .prepare("SELECT * FROM events WHERE actor_id = ? ORDER BY tick DESC")
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let events = stmt
            .query_map(params![actor_id], |row| {
                let event_id: String = row.get(1)?;
                let tick: u32 = row.get(2)?;
                let year: i32 = row.get(3)?;
                let actor_id: String = row.get(4)?;
                let event_type_str: String = row.get(5)?;
                let description: String = row.get(6)?;
                let metrics_snapshot_str: String = row.get(7)?;
                let involved_actors_str: String = row.get(8)?;
                let tags_str: String = row.get(9)?;
                let is_key: i32 = row.get(10)?;

                let event_type = Self::string_to_event_type(&event_type_str);
                let metrics_snapshot: HashMap<String, f64> =
                    serde_json::from_str(&metrics_snapshot_str).unwrap_or_default();
                let involved_actors: Vec<String> =
                    serde_json::from_str(&involved_actors_str).unwrap_or_default();
                let tags: Vec<String> =
                    serde_json::from_str(&tags_str).unwrap_or_default();
                let scenario_id: String = row.get(11).unwrap_or_default();

                Ok(Event {
                    id: event_id,
                    tick,
                    year,
                    actor_id,
                    event_type,
                    description,
                    metrics_snapshot,
                    involved_actors,
                    tags,
                    is_key: is_key != 0,
                    scenario_id,
                })
            })
            .map_err(|e| format!("Failed to query events: {}", e))?;

        let mut result = Vec::new();
        for event in events {
            match event {
                Ok(e) => result.push(e),
                Err(e) => eprintln!("Error parsing event: {}", e),
            }
        }

        Ok(result)
    }

    /// Get events within a tick range
    pub fn get_events_by_tick_range(
        &self,
        start_tick: u32,
        end_tick: u32,
    ) -> Result<Vec<Event>, String> {
        let mut stmt = self.conn
            .prepare("SELECT * FROM events WHERE tick >= ? AND tick <= ? ORDER BY tick DESC")
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let events = stmt
            .query_map(params![start_tick, end_tick], |row| {
                let event_id: String = row.get(1)?;
                let tick: u32 = row.get(2)?;
                let year: i32 = row.get(3)?;
                let actor_id: String = row.get(4)?;
                let event_type_str: String = row.get(5)?;
                let description: String = row.get(6)?;
                let metrics_snapshot_str: String = row.get(7)?;
                let involved_actors_str: String = row.get(8)?;
                let tags_str: String = row.get(9)?;
                let is_key: i32 = row.get(10)?;

                let event_type = Self::string_to_event_type(&event_type_str);
                let metrics_snapshot: HashMap<String, f64> =
                    serde_json::from_str(&metrics_snapshot_str).unwrap_or_default();
                let involved_actors: Vec<String> =
                    serde_json::from_str(&involved_actors_str).unwrap_or_default();
                let tags: Vec<String> =
                    serde_json::from_str(&tags_str).unwrap_or_default();
                let scenario_id: String = row.get(11).unwrap_or_default();

                Ok(Event {
                    id: event_id,
                    tick,
                    year,
                    actor_id,
                    event_type,
                    description,
                    metrics_snapshot,
                    involved_actors,
                    tags,
                    is_key: is_key != 0,
                    scenario_id,
                })
            })
            .map_err(|e| format!("Failed to query events: {}", e))?;

        let mut result = Vec::new();
        for event in events {
            match event {
                Ok(e) => result.push(e),
                Err(e) => eprintln!("Error parsing event: {}", e),
            }
        }

        Ok(result)
    }

    /// Get key events for a specific actor
    pub fn get_key_events_by_actor(&self, actor_id: &str) -> Result<Vec<Event>, String> {
        let mut stmt = self.conn
            .prepare("SELECT * FROM events WHERE actor_id = ? AND is_key = 1 ORDER BY tick DESC")
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let events = stmt
            .query_map(params![actor_id], |row| {
                let event_id: String = row.get(1)?;
                let tick: u32 = row.get(2)?;
                let year: i32 = row.get(3)?;
                let actor_id: String = row.get(4)?;
                let event_type_str: String = row.get(5)?;
                let description: String = row.get(6)?;
                let metrics_snapshot_str: String = row.get(7)?;
                let involved_actors_str: String = row.get(8)?;
                let tags_str: String = row.get(9)?;
                let is_key: i32 = row.get(10)?;

                let event_type = Self::string_to_event_type(&event_type_str);
                let metrics_snapshot: HashMap<String, f64> =
                    serde_json::from_str(&metrics_snapshot_str).unwrap_or_default();
                let involved_actors: Vec<String> =
                    serde_json::from_str(&involved_actors_str).unwrap_or_default();
                let tags: Vec<String> =
                    serde_json::from_str(&tags_str).unwrap_or_default();
                let scenario_id: String = row.get(11).unwrap_or_default();

                Ok(Event {
                    id: event_id,
                    tick,
                    year,
                    actor_id,
                    event_type,
                    description,
                    metrics_snapshot,
                    involved_actors,
                    tags,
                    is_key: is_key != 0,
                    scenario_id,
                })
            })
            .map_err(|e| format!("Failed to query events: {}", e))?;

        let mut result = Vec::new();
        for event in events {
            match event {
                Ok(e) => result.push(e),
                Err(e) => eprintln!("Error parsing event: {}", e),
            }
        }

        Ok(result)
    }

    /// Get relevant events with temporal decay and thematic scoring
    /// Returns events sorted by relevance (thematic_similarity × temporal_coefficient)
    pub fn get_relevant_events_scored(
        &self,
        current_tick: u32,
        query_tags: &[String],
        narrative_actor_ids: &[String],
    ) -> Result<Vec<Event>, String> {
        // Get all events for narrative actors plus key events
        let mut all_events: Vec<Event> = Vec::new();

        // Get events for each narrative actor
        for actor_id in narrative_actor_ids {
            let mut events = self.get_events_by_actor(actor_id)?;
            all_events.append(&mut events);
        }

        // Get all key events
        let key_events = self.get_all_key_events()?;
        for event in key_events {
            if !all_events.iter().any(|e| e.id == event.id) {
                all_events.push(event);
            }
        }

        // Calculate relevance score for each event
        let mut scored_events: Vec<(Event, f64)> = all_events
            .into_iter()
            .map(|event| {
                let ticks_ago = current_tick.saturating_sub(event.tick);
                let temporal_coeff = Self::temporal_coefficient(ticks_ago, event.is_key);
                let thematic_sim = Self::thematic_similarity(&event.tags, query_tags);
                let relevance = thematic_sim * temporal_coeff;
                (event, relevance)
            })
            .collect();

        // Sort by relevance descending
        scored_events.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Apply selection rules:
        // 1. Top 15 by relevance
        // 2. Last 5 events always (regardless of relevance)
        // 3. All is_key events from narrative actors always

        // Get last 5 events (most recent by tick)
        let mut by_tick = scored_events.clone();
        by_tick.sort_by(|a, b| b.0.tick.cmp(&a.0.tick));
        let _last_5: HashSet<String> = by_tick.iter().take(5).map(|(e, _)| e.id.clone()).collect();

        // Get all is_key events from narrative actors
        let key_from_narrative: HashSet<String> = scored_events
            .iter()
            .filter(|(e, _)| e.is_key && narrative_actor_ids.contains(&e.actor_id))
            .map(|(e, _)| e.id.clone())
            .collect();

        // Build final list with deduplication
        let mut final_events: Vec<(Event, f64)> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        // Add top 15 by relevance
        for (event, score) in scored_events.iter() {
            if final_events.len() >= 15 {
                break;
            }
            if !seen_ids.contains(&event.id) {
                seen_ids.insert(event.id.clone());
                final_events.push((event.clone(), *score));
            }
        }

        // Add last 5 (if not already included)
        for (event, score) in by_tick.iter().take(5) {
            if !seen_ids.contains(&event.id) {
                seen_ids.insert(event.id.clone());
                final_events.push((event.clone(), *score));
            }
        }

        // Add is_key events from narrative actors (if not already included)
        for (event, score) in scored_events.iter() {
            if key_from_narrative.contains(&event.id) && !seen_ids.contains(&event.id) {
                seen_ids.insert(event.id.clone());
                final_events.push((event.clone(), *score));
            }
        }

        // Sort final list by tick descending (most recent first) for presentation
        final_events.sort_by(|a, b| b.0.tick.cmp(&a.0.tick));

        // Return just the events (scores are logged for debugging)
        Ok(final_events.into_iter().map(|(e, _)| e).collect())
    }

    /// Get all key events from database
    fn get_all_key_events(&self) -> Result<Vec<Event>, String> {
        let mut stmt = self.conn
            .prepare("SELECT * FROM events WHERE is_key = 1 ORDER BY tick DESC")
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let events = stmt
            .query_map([], |row: &rusqlite::Row| {
                let event_id: String = row.get(1)?;
                let tick: u32 = row.get(2)?;
                let year: i32 = row.get(3)?;
                let actor_id: String = row.get(4)?;
                let event_type_str: String = row.get(5)?;
                let description: String = row.get(6)?;
                let metrics_snapshot_str: String = row.get(7)?;
                let involved_actors_str: String = row.get(8)?;
                let tags_str: String = row.get(9)?;
                let is_key: i32 = row.get(10)?;

                let event_type = Self::string_to_event_type(&event_type_str);
                let metrics_snapshot: HashMap<String, f64> =
                    serde_json::from_str(&metrics_snapshot_str).unwrap_or_default();
                let involved_actors: Vec<String> =
                    serde_json::from_str(&involved_actors_str).unwrap_or_default();
                let tags: Vec<String> =
                    serde_json::from_str(&tags_str).unwrap_or_default();
                let scenario_id: String = row.get(11).unwrap_or_default();

                Ok(Event {
                    id: event_id,
                    tick,
                    year,
                    actor_id,
                    event_type,
                    description,
                    metrics_snapshot,
                    involved_actors,
                    tags,
                    is_key: is_key != 0,
                    scenario_id,
                })
            })
            .map_err(|e| format!("Failed to query key events: {}", e))?;

        let mut result = Vec::new();
        for event in events {
            if let Ok(e) = event {
                result.push(e);
            }
        }

        Ok(result)
    }

    /// Calculate temporal coefficient based on ticks ago
    /// is_key events have minimum coefficient of 0.3
    pub fn temporal_coefficient(ticks_ago: u32, is_key: bool) -> f64 {
        let coeff: f64 = match ticks_ago {
            0..=10 => 1.0,
            11..=30 => 0.7,
            31..=60 => 0.4,
            61..=100 => 0.2,
            _ => 0.05,
        };

        // is_key events never drop below 0.3
        if is_key {
            coeff.max(0.3)
        } else {
            coeff
        }
    }

    /// Calculate thematic similarity between event tags and query tags
    /// similarity = matching_tags / max(event_tags.len(), query_tags.len())
    pub fn thematic_similarity(event_tags: &[String], query_tags: &[String]) -> f64 {
        if event_tags.is_empty() && query_tags.is_empty() {
            return 1.0;
        }

        let event_set: HashSet<&String> = event_tags.iter().collect();
        let query_set: HashSet<&String> = query_tags.iter().collect();

        let matching = event_set.intersection(&query_set).count() as f64;
        let max_len = event_tags.len().max(query_tags.len()) as f64;

        if max_len == 0.0 {
            1.0
        } else {
            matching / max_len
        }
    }

    // ========================================================================
    // Save operations
    // ========================================================================

    /// Insert or update a save
    pub fn insert_save(&self, save: &DbSave) -> Result<(), String> {
        self.conn
            .execute(
                "
                INSERT OR REPLACE INTO saves 
                (id, name, scenario_id, tick, year, created_at, world_state_json, player_state_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
                params![
                    save.id,
                    save.name,
                    save.scenario_id,
                    save.tick,
                    save.year,
                    save.created_at,
                    save.world_state_json,
                    save.player_state_json
                ],
            )
            .map_err(|e| format!("Failed to insert save: {}", e))?;

        Ok(())
    }

    /// Get a save by ID
    pub fn get_save_by_id(&self, save_id: &str) -> Result<Option<DbSave>, String> {
        let mut stmt = self.conn
            .prepare("SELECT * FROM saves WHERE id = ?")
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let save = stmt
            .query_row(params![save_id], |row| {
                Ok(DbSave {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    scenario_id: row.get(2)?,
                    tick: row.get(3)?,
                    year: row.get(4)?,
                    created_at: row.get(5)?,
                    world_state_json: row.get(6)?,
                    player_state_json: row.get(7)?,
                })
            })
            .optional()
            .map_err(|e| format!("Failed to query save: {}", e))?;

        Ok(save)
    }

    /// List all saves
    pub fn list_saves(&self) -> Result<Vec<DbSave>, String> {
        let mut stmt = self.conn
            .prepare("SELECT * FROM saves ORDER BY created_at DESC")
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let saves = stmt
            .query_map([], |row| {
                Ok(DbSave {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    scenario_id: row.get(2)?,
                    tick: row.get(3)?,
                    year: row.get(4)?,
                    created_at: row.get(5)?,
                    world_state_json: row.get(6)?,
                    player_state_json: row.get(7)?,
                })
            })
            .map_err(|e| format!("Failed to query saves: {}", e))?;

        let mut result = Vec::new();
        for save in saves {
            match save {
                Ok(s) => result.push(s),
                Err(e) => eprintln!("Error parsing save: {}", e),
            }
        }

        Ok(result)
    }

    /// List saves for a specific scenario
    pub fn list_saves_by_scenario(&self, scenario_id: &str) -> Result<Vec<DbSave>, String> {
        let mut stmt = self.conn
            .prepare("SELECT * FROM saves WHERE scenario_id = ? ORDER BY created_at DESC")
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let saves = stmt
            .query_map(params![scenario_id], |row| {
                Ok(DbSave {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    scenario_id: row.get(2)?,
                    tick: row.get(3)?,
                    year: row.get(4)?,
                    created_at: row.get(5)?,
                    world_state_json: row.get(6)?,
                    player_state_json: row.get(7)?,
                })
            })
            .map_err(|e| format!("Failed to query saves: {}", e))?;

        let mut result = Vec::new();
        for save in saves {
            match save {
                Ok(s) => result.push(s),
                Err(e) => eprintln!("Error parsing save: {}", e),
            }
        }

        Ok(result)
    }

    /// Delete a save
    pub fn delete_save(&self, save_id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM saves WHERE id = ?", params![save_id])
            .map_err(|e| format!("Failed to delete save: {}", e))?;

        Ok(())
    }

    // ========================================================================
    // Dead actor operations
    // ========================================================================

    /// Insert a dead actor from core::DeadActor
    pub fn insert_dead_actor_from_core(
        &self,
        dead_actor: &crate::core::DeadActor,
    ) -> Result<(), String> {
        let final_metrics_json = serde_json::to_string(&dead_actor.final_metrics)
            .map_err(|e| format!("Failed to serialize final metrics: {}", e))?;
        let successor_ids_json = serde_json::to_string(&dead_actor.successor_ids)
            .map_err(|e| format!("Failed to serialize successor ids: {}", e))?;

        let db_dead_actor = DbDeadActor {
            id: dead_actor.id.clone(),
            tick_death: dead_actor.tick_death,
            year_death: dead_actor.year_death,
            final_metrics_json,
            successor_ids_json,
        };

        self.insert_dead_actor(&db_dead_actor)
    }

    /// Insert a dead actor
    pub fn insert_dead_actor(&self, dead_actor: &DbDeadActor) -> Result<(), String> {
        self.conn
            .execute(
                "
                INSERT OR REPLACE INTO dead_actors 
                (id, tick_death, year_death, final_metrics_json, successor_ids_json)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    dead_actor.id,
                    dead_actor.tick_death,
                    dead_actor.year_death,
                    dead_actor.final_metrics_json,
                    dead_actor.successor_ids_json
                ],
            )
            .map_err(|e| format!("Failed to insert dead actor: {}", e))?;

        Ok(())
    }

    /// Get a dead actor by ID
    pub fn get_dead_actor(&self, actor_id: &str) -> Result<Option<DbDeadActor>, String> {
        let mut stmt = self.conn
            .prepare("SELECT * FROM dead_actors WHERE id = ?")
            .map_err(|e| format!("Failed to prepare statement: {}", e))?;

        let dead_actor = stmt
            .query_row(params![actor_id], |row| {
                Ok(DbDeadActor {
                    id: row.get(0)?,
                    tick_death: row.get(1)?,
                    year_death: row.get(2)?,
                    final_metrics_json: row.get(3)?,
                    successor_ids_json: row.get(4)?,
                })
            })
            .optional()
            .map_err(|e| format!("Failed to query dead actor: {}", e))?;

        Ok(dead_actor)
    }

    // ========================================================================
    // Helper functions
    // ========================================================================

    fn event_type_to_string(event_type: &crate::core::EventType) -> &'static str {
        match event_type {
            crate::core::EventType::Collapse => "collapse",
            crate::core::EventType::War => "war",
            crate::core::EventType::Migration => "migration",
            crate::core::EventType::Threshold => "threshold",
            crate::core::EventType::Birth => "birth",
            crate::core::EventType::Death => "death",
            crate::core::EventType::Trade => "trade",
            crate::core::EventType::Cultural => "cultural",
            crate::core::EventType::Diplomatic => "diplomatic",
            crate::core::EventType::PlayerAction => "player_action",
            crate::core::EventType::Milestone => "milestone",
        }
    }

    fn string_to_event_type(s: &str) -> crate::core::EventType {
        match s {
            "collapse" => crate::core::EventType::Collapse,
            "war" => crate::core::EventType::War,
            "migration" => crate::core::EventType::Migration,
            "threshold" => crate::core::EventType::Threshold,
            "birth" => crate::core::EventType::Birth,
            "death" => crate::core::EventType::Death,
            "trade" => crate::core::EventType::Trade,
            "cultural" => crate::core::EventType::Cultural,
            "diplomatic" => crate::core::EventType::Diplomatic,
            "player_action" => crate::core::EventType::PlayerAction,
            "milestone" => crate::core::EventType::Milestone,
            _ => crate::core::EventType::Threshold,
        }
    }
}
