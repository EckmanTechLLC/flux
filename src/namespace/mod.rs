use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rand::Rng;
use std::sync::Arc;
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// Namespace represents a user's isolated space in Flux
#[derive(Debug, Clone, PartialEq)]
pub struct Namespace {
    /// System-generated ID (ns_{random_8chars})
    pub id: String,
    /// User-chosen name (unique, 3-32 chars, [a-z0-9-_])
    pub name: String,
    /// Bearer token for write authorization (UUID v4)
    pub token: String,
    /// When namespace was created
    pub created_at: DateTime<Utc>,
    /// Number of entities in this namespace (stats)
    pub entity_count: u64,
}

/// Namespace registry manages registration and lookups
pub struct NamespaceRegistry {
    /// Primary storage: namespace_id -> Namespace
    namespaces: Arc<DashMap<String, Namespace>>,
    /// Secondary index: name -> namespace_id (for uniqueness)
    names: Arc<DashMap<String, String>>,
    /// Secondary index: token -> namespace_id (for auth)
    tokens: Arc<DashMap<String, String>>,
}

impl NamespaceRegistry {
    /// Create new empty registry
    pub fn new() -> Self {
        Self {
            namespaces: Arc::new(DashMap::new()),
            names: Arc::new(DashMap::new()),
            tokens: Arc::new(DashMap::new()),
        }
    }

    /// Register a new namespace with given name
    ///
    /// Returns the created Namespace with generated ID and token.
    /// Fails if name is invalid or already exists.
    pub fn register(&self, name: &str) -> Result<Namespace, RegistrationError> {
        // Validate name format
        Self::validate_name(name)?;

        // Check uniqueness
        if self.names.contains_key(name) {
            return Err(RegistrationError::NameAlreadyExists);
        }

        // Generate namespace ID and token
        let namespace_id = generate_namespace_id();
        let token = Uuid::new_v4().to_string();
        let now = Utc::now();

        // Create namespace
        let namespace = Namespace {
            id: namespace_id.clone(),
            name: name.to_string(),
            token: token.clone(),
            created_at: now,
            entity_count: 0,
        };

        // Insert into all indices
        self.namespaces
            .insert(namespace_id.clone(), namespace.clone());
        self.names.insert(name.to_string(), namespace_id.clone());
        self.tokens.insert(token.clone(), namespace_id);

        Ok(namespace)
    }

    /// Validate namespace name format
    ///
    /// Rules: 3-32 characters, lowercase alphanumeric + dash/underscore
    pub fn validate_name(name: &str) -> Result<(), ValidationError> {
        let len = name.len();

        // Length check
        if len < 3 {
            return Err(ValidationError::TooShort);
        }
        if len > 32 {
            return Err(ValidationError::TooLong);
        }

        // Character check: [a-z0-9-_]
        for c in name.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '_' {
                return Err(ValidationError::InvalidCharacters(format!(
                    "Invalid character '{}' (must be [a-z0-9-_])",
                    c
                )));
            }
        }

        Ok(())
    }

    /// Look up namespace by name
    pub fn lookup_by_name(&self, name: &str) -> Option<Namespace> {
        let namespace_id = self.names.get(name)?;
        self.namespaces.get(namespace_id.value()).map(|n| n.clone())
    }

    /// Look up namespace by token
    pub fn lookup_by_token(&self, token: &str) -> Option<Namespace> {
        let namespace_id = self.tokens.get(token)?;
        self.namespaces.get(namespace_id.value()).map(|n| n.clone())
    }

    /// Validate that a token owns a namespace
    ///
    /// Used for write authorization: checks if the given token
    /// is authorized to write to the given namespace name.
    pub fn validate_token(&self, token: &str, namespace: &str) -> Result<(), AuthError> {
        // Look up namespace by name
        let ns = self
            .lookup_by_name(namespace)
            .ok_or(AuthError::NamespaceNotFound)?;

        // Check token match
        if ns.token != token {
            return Err(AuthError::Unauthorized);
        }

        Ok(())
    }

    /// Get namespace by ID (internal use)
    pub fn get(&self, namespace_id: &str) -> Option<Namespace> {
        self.namespaces.get(namespace_id).map(|n| n.clone())
    }

    /// Get count of registered namespaces
    pub fn count(&self) -> usize {
        self.namespaces.len()
    }
}

impl Default for NamespaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate namespace ID: ns_{random_8chars}
fn generate_namespace_id() -> String {
    let mut rng = rand::thread_rng();
    let random: String = (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect();
    format!("ns_{}", random)
}

/// Registration errors
#[derive(Debug, PartialEq)]
pub enum RegistrationError {
    InvalidName(ValidationError),
    NameAlreadyExists,
}

impl From<ValidationError> for RegistrationError {
    fn from(e: ValidationError) -> Self {
        RegistrationError::InvalidName(e)
    }
}

/// Name validation errors
#[derive(Debug, PartialEq)]
pub enum ValidationError {
    TooShort,
    TooLong,
    InvalidCharacters(String),
}

/// Authorization errors
#[derive(Debug, PartialEq)]
pub enum AuthError {
    NamespaceNotFound,
    Unauthorized,
}
