use crate::cache::Cache;
use crate::config::Config;
use crate::journal::{Action, Journal};
use crate::model::{CalendarListEntry, Task, TaskStatus};
use crate::storage::{LOCAL_CALENDAR_HREF, LocalStorage};

// Libdav imports
use libdav::caldav::{FindCalendarHomeSet, FindCalendars, GetCalendarResources};
use libdav::dav::{Delete, GetProperty, ListResources, PutResource};
use libdav::dav::{WebDavClient, WebDavError};
use libdav::{CalDavClient, names};

use futures::stream::{self, StreamExt};
use http::{Request, StatusCode, Uri};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use rustls_native_certs;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::auth::AddAuthorization;
use uuid::Uuid;

type HttpsClient = AddAuthorization<
    Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        String,
    >,
>;

#[derive(Clone, Debug)]
pub struct RustyClient {
    client: Option<CalDavClient<HttpsClient>>,
}

impl RustyClient {
    pub fn new(url: &str, user: &str, pass: &str, insecure: bool) -> Result<Self, String> {
        if url.is_empty() {
            return Ok(Self { client: None });
        }

        let uri: Uri = url
            .parse()
            .map_err(|e: http::uri::InvalidUri| e.to_string())?;

        let https_connector = if insecure {
            let tls_config = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerifier))
                .with_no_client_auth();

            HttpsConnectorBuilder::new()
                .with_tls_config(tls_config)
                .https_or_http()
                .enable_http1()
                .build()
        } else {
            let mut root_store = rustls::RootCertStore::empty();
            let result = rustls_native_certs::load_native_certs();
            root_store.add_parsable_certificates(result.certs);

            if root_store.is_empty() {
                return Err("No valid system certificates found.".to_string());
            }

            let tls_config = rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();

            HttpsConnectorBuilder::new()
                .with_tls_config(tls_config)
                .https_or_http()
                .enable_http1()
                .build()
        };

        let http_client = Client::builder(TokioExecutor::new()).build(https_connector);
        let auth_client = AddAuthorization::basic(http_client.clone(), user, pass);
        let webdav = WebDavClient::new(uri, auth_client.clone());
        let caldav = CalDavClient::new(webdav);

        Ok(Self {
            client: Some(caldav),
        })
    }

    pub async fn discover_calendar(&self) -> Result<String, String> {
        if let Some(client) = &self.client {
            let base_path = client.base_url().path().to_string();

            // 1. Try generic WebDAV list to see if base path is already a calendar
            if let Ok(response) = client.request(ListResources::new(&base_path)).await
                && response.resources.iter().any(|r| r.href.ends_with(".ics"))
            {
                return Ok(base_path);
            }

            // 2. Try CalDAV Discovery
            if let Ok(Some(principal)) = client.find_current_user_principal().await {
                if let Ok(response) = client.request(FindCalendarHomeSet::new(&principal)).await
                    && let Some(home_url) = response.home_sets.first()
                {
                    if let Ok(cals_resp) = client.request(FindCalendars::new(home_url)).await
                        && let Some(first) = cals_resp.calendars.first()
                    {
                        return Ok(first.href.clone());
                    }
                }
            }
            Ok(base_path)
        } else {
            Err("Offline".to_string())
        }
    }

    pub async fn connect_with_fallback(
        config: Config,
    ) -> Result<
        (
            Self,
            Vec<CalendarListEntry>,
            Vec<Task>,
            Option<String>,
            Option<String>,
        ),
        String,
    > {
        let client = Self::new(
            &config.url,
            &config.username,
            &config.password,
            config.allow_insecure_certs,
        )
        .map_err(|e| e.to_string())?;
        let _ = client.sync_journal().await;
        let (calendars, warning) = match client.get_calendars().await {
            Ok(c) => {
                let _ = Cache::save_calendars(&c);
                (c, None)
            }
            Err(e) => {
                if e.contains("InvalidCertificate") {
                    return Err(format!("Connection failed: {}", e));
                }
                (
                    Cache::load_calendars().unwrap_or_default(),
                    Some("Offline Mode".to_string()),
                )
            }
        };
        let mut active_href = None;
        if let Some(def_cal) = &config.default_calendar
            && let Some(found) = calendars
                .iter()
                .find(|c| c.name == *def_cal || c.href == *def_cal)
        {
            active_href = Some(found.href.clone());
        }
        if active_href.is_none()
            && warning.is_none()
            && let Ok(href) = client.discover_calendar().await
        {
            active_href = Some(href);
        }
        let tasks = if warning.is_none() {
            if let Some(ref h) = active_href {
                client.get_tasks(h).await.unwrap_or_default()
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        Ok((client, calendars, tasks, active_href, warning))
    }

    pub async fn get_calendars(&self) -> Result<Vec<CalendarListEntry>, String> {
        if let Some(client) = &self.client {
            let principal = client
                .find_current_user_principal()
                .await
                .map_err(|e| format!("{:?}", e))?
                .ok_or("No principal")?;

            let home_set_resp = client
                .request(FindCalendarHomeSet::new(&principal))
                .await
                .map_err(|e| format!("{:?}", e))?;
            let home_url = home_set_resp.home_sets.first().ok_or("No home set")?;

            let cals_resp = client
                .request(FindCalendars::new(home_url))
                .await
                .map_err(|e| format!("{:?}", e))?;

            let mut calendars = Vec::new();
            for col in cals_resp.calendars {
                let name = client
                    .request(GetProperty::new(&col.href, &names::DISPLAY_NAME))
                    .await
                    .ok()
                    .and_then(|r| r.value)
                    .unwrap_or_else(|| col.href.clone());

                calendars.push(CalendarListEntry {
                    name,
                    href: col.href,
                    color: None,
                });
            }
            Ok(calendars)
        } else {
            Ok(vec![])
        }
    }

    pub async fn get_tasks(&self, calendar_href: &str) -> Result<Vec<Task>, String> {
        if calendar_href == LOCAL_CALENDAR_HREF {
            return LocalStorage::load().map_err(|e| e.to_string());
        }
        if let Some(client) = &self.client {
            let _ = self.sync_journal().await;

            let list_resp = client
                .request(ListResources::new(calendar_href))
                .await
                .map_err(|e| format!("PROPFIND: {:?}", e))?;

            let cached_tasks = Cache::load(calendar_href).unwrap_or_default();
            let mut cache_map: HashMap<String, Task> = HashMap::new();
            for t in cached_tasks {
                cache_map.insert(t.href.clone(), t);
            }

            let mut final_tasks = Vec::new();
            let mut to_fetch = Vec::new();

            for resource in list_resp.resources {
                // Ignore collection itself or non-ics
                if !resource.href.ends_with(".ics") {
                    continue;
                }

                let remote_etag = resource.etag;

                if let Some(local_task) = cache_map.remove(&resource.href) {
                    if let Some(r_etag) = &remote_etag
                        && !r_etag.is_empty()
                        && *r_etag == local_task.etag
                    {
                        final_tasks.push(local_task);
                    } else {
                        to_fetch.push(resource.href);
                    }
                } else {
                    to_fetch.push(resource.href);
                }
            }

            if !to_fetch.is_empty() {
                // Use calendar-multiget
                let fetched_resp = client
                    .request(GetCalendarResources::new(calendar_href).with_hrefs(to_fetch))
                    .await
                    .map_err(|e| format!("MULTIGET: {:?}", e))?;

                for item in fetched_resp.resources {
                    if let Ok(content) = item.content {
                        if let Ok(task) = Task::from_ics(
                            &content.data,
                            content.etag,
                            item.href,
                            calendar_href.to_string(),
                        ) {
                            final_tasks.push(task);
                        }
                    }
                }
            }
            Ok(final_tasks)
        } else {
            Err("Offline".to_string())
        }
    }

    pub async fn get_all_tasks(
        &self,
        calendars: &[CalendarListEntry],
    ) -> Result<Vec<(String, Vec<Task>)>, String> {
        let hrefs: Vec<String> = calendars.iter().map(|c| c.href.clone()).collect();
        let futures = hrefs.into_iter().map(|href| {
            let client = self.clone();
            async move { (href.clone(), client.get_tasks(&href).await) }
        });
        let mut stream = stream::iter(futures).buffer_unordered(4);
        let mut final_results = Vec::new();
        while let Some((href, res)) = stream.next().await {
            if let Ok(tasks) = res {
                final_results.push((href, tasks));
            }
        }
        Ok(final_results)
    }

    pub async fn create_task(&self, task: &mut Task) -> Result<(), String> {
        if task.calendar_href == LOCAL_CALENDAR_HREF {
            let mut all = LocalStorage::load().unwrap_or_default();
            all.push(task.clone());
            LocalStorage::save(&all).map_err(|e| e.to_string())?;
            return Ok(());
        }
        let filename = format!("{}.ics", task.uid);
        let full_href = if task.calendar_href.ends_with('/') {
            format!("{}{}", task.calendar_href, filename)
        } else {
            format!("{}/{}", task.calendar_href, filename)
        };
        task.href = full_href;

        Journal::push(Action::Create(task.clone())).map_err(|e| e.to_string())
    }

    pub async fn update_task(&self, task: &mut Task) -> Result<(), String> {
        if task.calendar_href == LOCAL_CALENDAR_HREF {
            let mut all = LocalStorage::load().unwrap_or_default();
            if let Some(idx) = all.iter().position(|t| t.uid == task.uid) {
                all[idx] = task.clone();
                LocalStorage::save(&all).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        Journal::push(Action::Update(task.clone())).map_err(|e| e.to_string())
    }

    pub async fn delete_task(&self, task: &Task) -> Result<(), String> {
        if task.calendar_href == LOCAL_CALENDAR_HREF {
            let mut all = LocalStorage::load().unwrap_or_default();
            all.retain(|t| t.uid != task.uid);
            LocalStorage::save(&all).map_err(|e| e.to_string())?;
            return Ok(());
        }
        Journal::push(Action::Delete(task.clone())).map_err(|e| e.to_string())
    }

    pub async fn toggle_task(&self, task: &mut Task) -> Result<(Task, Option<Task>), String> {
        if task.status == TaskStatus::Completed {
            task.status = TaskStatus::NeedsAction;
        } else {
            task.status = TaskStatus::Completed;
        }
        let next_task = if task.status == TaskStatus::Completed {
            task.respawn()
        } else {
            None
        };

        if task.calendar_href == LOCAL_CALENDAR_HREF {
            let mut all = LocalStorage::load().unwrap_or_default();
            if let Some(idx) = all.iter().position(|t| t.uid == task.uid) {
                all[idx] = task.clone();
            }
            if let Some(new_t) = &next_task {
                all.push(new_t.clone());
            }
            LocalStorage::save(&all).map_err(|e| e.to_string())?;
            return Ok((task.clone(), next_task));
        }

        if let Some(mut next) = next_task.clone() {
            self.create_task(&mut next).await?;
        }
        self.update_task(task).await?;
        Ok((task.clone(), next_task))
    }

    pub async fn move_task(&self, task: &Task, new_calendar_href: &str) -> Result<Task, String> {
        if task.calendar_href == LOCAL_CALENDAR_HREF {
            let mut new_task = task.clone();
            new_task.calendar_href = new_calendar_href.to_string();
            new_task.href = String::new();
            new_task.etag = String::new();
            self.create_task(&mut new_task).await?;
            self.delete_task(task).await?;
            return Ok(new_task);
        }

        Journal::push(Action::Move(task.clone(), new_calendar_href.to_string()))
            .map_err(|e| e.to_string())?;

        let mut t = task.clone();
        t.calendar_href = new_calendar_href.to_string();
        Ok(t)
    }

    pub async fn migrate_tasks(
        &self,
        tasks: Vec<Task>,
        target_calendar_href: &str,
    ) -> Result<usize, String> {
        let mut success_count = 0;
        for task in tasks {
            if self.move_task(&task, target_calendar_href).await.is_ok() {
                success_count += 1;
            }
        }
        Ok(success_count)
    }

    pub async fn sync_journal(&self) -> Result<(), String> {
        let mut journal = Journal::load();
        if journal.is_empty() {
            return Ok(());
        }

        let client = self.client.as_ref().ok_or("Offline")?;

        while !journal.is_empty() {
            let action = journal.queue.remove(0);
            let mut conflict_resolved_action = None;

            let result = match &action {
                Action::Create(task) => {
                    let filename = format!("{}.ics", task.uid);
                    let full_href = if task.calendar_href.ends_with('/') {
                        format!("{}{}", task.calendar_href, filename)
                    } else {
                        format!("{}/{}", task.calendar_href, filename)
                    };
                    // PutResource::new(href).create(data, content_type)
                    let ics_string = task.to_ics();
                    match client
                        .request(PutResource::new(&full_href).create(ics_string, "text/calendar"))
                        .await
                    {
                        Ok(_) => Ok(()),
                        Err(e) => Err(format!("{:?}", e)),
                    }
                }
                Action::Update(task) => {
                    let ics_string = task.to_ics();
                    // PutResource::new(href).update(data, content_type, etag)
                    match client
                        .request(PutResource::new(&task.href).update(
                            ics_string,
                            "text/calendar; charset=utf-8; component=VTODO",
                            &task.etag,
                        ))
                        .await
                    {
                        Ok(_) => Ok(()),
                        Err(WebDavError::BadStatusCode(StatusCode::PRECONDITION_FAILED))
                        | Err(WebDavError::PreconditionFailed(_)) => {
                            // 412: CONFLICT detected
                            println!("Conflict on task {}. Creating copy.", task.uid);
                            let mut conflict_copy = task.clone();
                            conflict_copy.uid = Uuid::new_v4().to_string();
                            conflict_copy.summary = format!("{} (Conflict Copy)", task.summary);
                            conflict_copy.href = String::new();
                            conflict_copy.etag = String::new();
                            conflict_resolved_action = Some(Action::Create(conflict_copy));
                            Ok(())
                        }
                        Err(WebDavError::BadStatusCode(StatusCode::NOT_FOUND)) => {
                            conflict_resolved_action = Some(Action::Create(task.clone()));
                            Ok(())
                        }
                        Err(e) => Err(format!("{:?}", e)),
                    }
                }
                Action::Delete(task) => {
                    // Delete::new(href).with_etag(etag)
                    match client
                        .request(Delete::new(&task.href).with_etag(&task.etag))
                        .await
                    {
                        Ok(_) => Ok(()),
                        Err(WebDavError::BadStatusCode(StatusCode::NOT_FOUND)) => Ok(()),
                        Err(WebDavError::BadStatusCode(StatusCode::PRECONDITION_FAILED)) => {
                            // Etag mismatch on delete - just force delete or ignore?
                            // Safe route: Ignore, assume it changed and user can delete again if they see it.
                            println!("Conflict on delete task {}. Ignoring.", task.uid);
                            Ok(())
                        }
                        Err(e) => Err(format!("{:?}", e)),
                    }
                }
                Action::Move(task, new_cal) => self.execute_move(task, new_cal).await,
            };

            match result {
                Ok(_) => {
                    if let Some(act) = conflict_resolved_action {
                        let _ = journal.push_front(act);
                    }
                    journal.save().map_err(|e| e.to_string())?;
                }
                Err(e) => {
                    eprintln!("Sync Error: {}. Stopping sync.", e);
                    let _ = journal.push_front(action);
                    journal.save().map_err(|e| e.to_string())?;
                    break;
                }
            }
        }
        Ok(())
    }

    async fn execute_move(&self, task: &Task, new_calendar_href: &str) -> Result<(), String> {
        let client = self.client.as_ref().ok_or("Offline")?;

        let destination = if new_calendar_href.ends_with('/') {
            format!("{}{}.ics", new_calendar_href, task.uid)
        } else {
            format!("{}/{}.ics", new_calendar_href, task.uid)
        };

        // Construct raw HTTP request for MOVE using the underlying WebDavClient
        // We access the underlying client via Deref or direct field access depending on libdav implementation.
        // CalDavClient derefs to WebDavClient.

        let req = Request::builder()
            .method("MOVE")
            .uri(&task.href)
            .header("Destination", &destination)
            .header("Overwrite", "F")
            .body(String::new())
            .map_err(|e| e.to_string())?;

        let (parts, _) = client
            .webdav_client
            .request_raw(req)
            .await
            .map_err(|e| format!("{:?}", e))?;

        if parts.status.is_success() {
            Ok(())
        } else {
            Err(format!("MOVE failed: {}", parts.status))
        }
    }
}

#[derive(Debug)]
struct NoVerifier;
impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        use rustls::SignatureScheme::*;
        vec![
            RSA_PKCS1_SHA256,
            RSA_PKCS1_SHA384,
            RSA_PKCS1_SHA512,
            ECDSA_NISTP256_SHA256,
            RSA_PSS_SHA256,
            ED25519,
        ]
    }
}
