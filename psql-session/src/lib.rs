use chrono::{DateTime, SecondsFormat, Utc};
use mqtt_server::session::{Session, SessionError, SessionProvider};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::{Pool, Postgres};
use std::borrow::Cow;
use std::error::Error;

pub struct PsqlSession {
    client_identifier: String,
    pool: Pool<Postgres>,
}

#[derive(Deserialize, Serialize)]
struct EndTimestamp {
    end_timestamp: Option<String>,
}

#[async_trait::async_trait]
impl Session for PsqlSession {
    async fn clear(&mut self) -> Result<(), SessionError> {
        let results = sqlx::query!(
            "DELETE FROM sessions WHERE client_identifier = $1;",
            &self.client_identifier
        )
        .execute(&self.pool)
        .await
        .expect("foo");
        Ok(())
    }

    async fn set_endtimestamp(
        &mut self,
        timestamp: Option<DateTime<Utc>>,
    ) -> Result<(), SessionError> {
        let timestamp_str = timestamp.map(|t| t.to_rfc3339_opts(SecondsFormat::Nanos, false));
        let end_timestamp = EndTimestamp {
            end_timestamp: timestamp_str,
        };
        sqlx::query!(
            r"
            INSERT INTO sessions (client_identifier, session_data)
            VALUES ($2, $1)
            ON CONFLICT (client_identifier) DO
            UPDATE SET session_data = EXCLUDED.session_data || sessions.session_data
            WHERE sessions.client_identifier = $2;",
            Json(end_timestamp) as _,
            &self.client_identifier
        )
        .execute(&self.pool)
        .await
        .expect("set_endtimestamp");
        Ok(())
    }

    async fn get_endtimestamp(&self) -> Result<Option<Cow<DateTime<Utc>>>, SessionError> {
        todo!()
    }
}

pub struct PsqlSessionProviderBuilder {
    pool: Pool<Postgres>,
}

impl PsqlSessionProviderBuilder {
    pub fn new(pool: Pool<Postgres>) -> PsqlSessionProviderBuilder {
        PsqlSessionProviderBuilder { pool }
    }

    pub async fn migrate(&self) -> Result<(), Box<dyn Error>> {
        sqlx::migrate!().run(&self.pool).await?;
        Ok(())
    }

    pub fn build(self) -> Result<PsqlSessionProvider, ()> {
        Ok(PsqlSessionProvider { pool: self.pool })
    }
}

pub struct PsqlSessionProvider {
    pool: Pool<Postgres>,
}

#[async_trait::async_trait]
impl SessionProvider for PsqlSessionProvider {
    type S = PsqlSession;

    async fn get(&self, client_id: &str) -> Result<Self::S, SessionError> {
        Ok(PsqlSession {
            pool: self.pool.clone(),
            client_identifier: client_id.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::PsqlSessionProviderBuilder;
    use chrono::{DateTime, Utc};
    use mqtt_server::session::{Session, SessionProvider};
    use sqlx::error::UnexpectedNullError;
    use sqlx::migrate::MigrateDatabase;
    use sqlx::pool::PoolConnection;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{ConnectOptions, PgPool, Pool};
    use std::env;
    use std::error::Error;
    use std::str::FromStr;

    #[tokio::test]
    pub async fn test_builder() -> Result<(), Box<dyn Error>> {
        let database_url = env::var("DATABASE_URL")?;
        let s = sqlx::postgres::PgConnectOptions::from_str(&database_url)?;
        let s = s.database("postgres");
        let mut c = s.connect().await?;
        sqlx::query!("CREATE DATABASE havartester;")
            .execute(&mut c)
            .await?;

        {
            let s = s.clone();
            let s = s.database("havartester");
            let mut p = PgPool::connect_with(s).await?;

            let builder = PsqlSessionProviderBuilder::new(p.clone());
            builder.migrate().await?;

            /*
            sqlx::query!("INSERT INTO sessions (client_identifier) VALUES ('foo');")
                .execute(&mut p.acquire().await?)
                .await?;
             */

            let sessionProvider = builder
                .build()
                .map_err(|_| Box::new(sqlx::error::UnexpectedNullError))?;
            let mut session = sessionProvider
                .get("foo")
                .await
                .map_err(|_| Box::new(sqlx::error::UnexpectedNullError))?;
            session.set_endtimestamp(Some(Utc::now())).await;
            //session.clear().await;

            let res = sqlx::query!("SELECT COUNT(*) AS count FROM sessions;")
                .fetch_one(&mut p.acquire().await?)
                .await?;

            assert_eq!(0, res.count.expect("foo"));

            p.close().await;
        }

        sqlx::query!("DROP DATABASE havartester;")
            .execute(&mut c)
            .await?;

        /*
            let database_url = env::var("DATABASE_URL")?;
            let s = sqlx::postgres::PgConnectOptions::from_str(&database_url)?;
            let s = s.database("havartester");
            let pool = PgPool::connect_with(s).await?;

            /*
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;
         */

            let builder = PsqlSessionProviderBuilder::new(pool);

            builder.migrate().await?;

            let x = builder.build().map_err(|_| Box::new(sqlx::error::UnexpectedNullError))?;

        sqlx::Postgres::drop_database("havartester").await?;

         */

        Ok(())
    }
}
