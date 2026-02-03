use chrono::{DateTime, Local, NaiveDateTime};
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::config::Config;
use crate::error::{GymSniperError, Result};

#[derive(Clone)]
pub struct PerfectGymClient {
    client: Client,
    config: Config,
    token: Arc<RwLock<Option<String>>>,
}

#[derive(Debug, Serialize)]
struct LoginRequest {
    #[serde(rename = "RememberMe")]
    remember_me: bool,
    #[serde(rename = "Login")]
    login: String,
    #[serde(rename = "Password")]
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    #[serde(rename = "User")]
    user: Option<UserInfo>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    #[serde(rename = "Member")]
    member: Option<MemberInfo>,
}

#[derive(Debug, Deserialize)]
struct MemberInfo {
    #[serde(rename = "Id")]
    id: u64,
    #[serde(rename = "FirstName")]
    first_name: String,
}

#[derive(Debug, Serialize)]
struct WeeklyClassesRequest {
    #[serde(rename = "clubId")]
    club_id: u32,
    #[serde(rename = "categoryId")]
    category_id: Option<u32>,
    #[serde(rename = "daysInWeek")]
    days_in_week: u32,
}

#[derive(Debug, Deserialize)]
struct WeeklyClassesResponse {
    #[serde(rename = "CalendarData")]
    calendar_data: Vec<ZoneData>,
}

#[derive(Debug, Deserialize)]
struct ZoneData {
    #[serde(rename = "ZoneName")]
    zone_name: String,
    #[serde(rename = "ClassesPerHour")]
    classes_per_hour: Vec<HourData>,
}

#[derive(Debug, Deserialize)]
struct HourData {
    #[serde(rename = "ClassesPerDay")]
    classes_per_day: Vec<Vec<ClassItem>>,
}

#[derive(Debug, Deserialize)]
struct ClassItem {
    #[serde(rename = "Id")]
    id: u64,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "StartTime")]
    start_time: String,
    #[serde(rename = "Duration")]
    duration: String,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Trainer")]
    trainer: Option<String>,
}

#[derive(Debug, Serialize)]
struct BookClassRequest {
    #[serde(rename = "classId")]
    class_id: u64,
    #[serde(rename = "clubId")]
    club_id: String,
}

#[derive(Debug, Deserialize)]
struct BookClassResponse {
    #[serde(rename = "Tickets")]
    tickets: Vec<BookingTicket>,
    #[serde(rename = "ClassId")]
    class_id: u64,
}

#[derive(Debug, Deserialize)]
struct BookingTicket {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "StartTime")]
    start_time: String,
    #[serde(rename = "Trainer")]
    trainer: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub id: u64,
    pub name: String,
    pub start_time: DateTime<Local>,
    pub status: String,
    pub trainer: Option<String>,
}

#[derive(Debug)]
pub struct BookingResult {
    pub name: String,
    pub start_time: DateTime<Local>,
    pub trainer: Option<String>,
}

// Class details response structures
#[derive(Debug, Deserialize)]
struct ClassDetailsResponse {
    #[serde(rename = "Id")]
    id: u64,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "StartTime")]
    start_time: String,
    #[serde(rename = "Trainer")]
    trainer: Option<String>,
    #[serde(rename = "Users")]
    users: Vec<ClassUser>,
}

#[derive(Debug, Deserialize)]
struct ClassUser {
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "StandByQueueNumber")]
    standby_queue_number: Option<u32>,
    #[serde(rename = "User")]
    user: ClassUserInfo,
}

#[derive(Debug, Deserialize)]
struct ClassUserInfo {
    #[serde(rename = "IsCurrentUser")]
    is_current_user: bool,
}

#[derive(Debug)]
pub struct MyBooking {
    pub id: u64,
    pub name: String,
    pub start_time: DateTime<Local>,
    pub status: String,
    pub waitlist_position: Option<u32>,
    pub trainer: Option<String>,
}

// Browser-like headers to appear more natural
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:146.0) Gecko/20100101 Firefox/146.0";

impl PerfectGymClient {
    pub fn new(config: &Config) -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::USER_AGENT, USER_AGENT.parse().unwrap());
        headers.insert(header::ACCEPT_LANGUAGE, "en-GB,en;q=0.5".parse().unwrap());

        let client = Client::builder()
            .cookie_store(true)
            .default_headers(headers)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            config: config.clone(),
            token: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn login(self) -> Result<Self> {
        let url = format!("{}/Auth/Login", self.config.gym.base_url);

        let request = LoginRequest {
            remember_me: false,
            login: self.config.credentials.email.clone(),
            password: self.config.credentials.password.clone(),
        };

        debug!("Logging in to {}", url);

        let origin = &self.config.gym.base_url.replace("/clientportal2", "");
        let referer = format!("{}/", self.config.gym.base_url);

        let response = self
            .client
            .post(&url)
            .header(header::CONTENT_TYPE, "application/json;charset=utf-8")
            .header(header::ACCEPT, "application/json, text/plain, */*")
            .header(header::ORIGIN, origin)
            .header(header::REFERER, &referer)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("CP-LANG", "en")
            .header("CP-MODE", "desktop")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(GymSniperError::Auth(format!(
                "Login failed with status: {}",
                response.status()
            )));
        }

        // Extract JWT token from response header
        let token = response
            .headers()
            .get("jwt-token")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if token.is_none() {
            return Err(GymSniperError::Auth(
                "No JWT token in login response".to_string(),
            ));
        }

        let login_response: LoginResponse = response.json().await?;

        if let Some(user) = login_response.user {
            if let Some(member) = user.member {
                debug!("Logged in as {} (ID: {})", member.first_name, member.id);
            }
        }

        *self.token.write().await = token;

        Ok(self)
    }

    pub async fn get_weekly_classes(&self, days: u32) -> Result<Vec<ClassInfo>> {
        let url = format!(
            "{}/Classes/ClassCalendar/WeeklyClasses",
            self.config.gym.base_url
        );

        let request = WeeklyClassesRequest {
            club_id: self.config.gym.club_id,
            category_id: None,
            days_in_week: days,
        };

        let token = self.token.read().await;
        let token = token
            .as_ref()
            .ok_or_else(|| GymSniperError::Auth("Not logged in".to_string()))?;

        let origin = &self.config.gym.base_url.replace("/clientportal2", "");
        let referer = format!("{}/", self.config.gym.base_url);

        let response = self
            .client
            .post(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .header(header::CONTENT_TYPE, "application/json;charset=utf-8")
            .header(header::ACCEPT, "application/json, text/plain, */*")
            .header(header::ORIGIN, origin)
            .header(header::REFERER, &referer)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("CP-LANG", "en")
            .header("CP-MODE", "desktop")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(GymSniperError::Api(format!(
                "Failed to get classes: {}",
                response.status()
            )));
        }

        let weekly_response: WeeklyClassesResponse = response.json().await?;

        let mut classes = Vec::new();
        for zone in weekly_response.calendar_data {
            for hour in zone.classes_per_hour {
                for day_classes in hour.classes_per_day {
                    for class in day_classes {
                        if let Ok(class_info) = parse_class_item(class) {
                            classes.push(class_info);
                        }
                    }
                }
            }
        }

        // Sort by start time
        classes.sort_by_key(|c| c.start_time);

        Ok(classes)
    }

    pub async fn book_class(&self, class_id: u64) -> Result<BookingResult> {
        let url = format!(
            "{}/Classes/ClassCalendar/BookClass",
            self.config.gym.base_url
        );

        let request = BookClassRequest {
            class_id,
            club_id: self.config.gym.club_id.to_string(),
        };

        let token = self.token.read().await;
        let token = token
            .as_ref()
            .ok_or_else(|| GymSniperError::Auth("Not logged in".to_string()))?;

        let origin = &self.config.gym.base_url.replace("/clientportal2", "");
        let referer = format!("{}/", self.config.gym.base_url);

        let response = self
            .client
            .post(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .header(header::CONTENT_TYPE, "application/json;charset=utf-8")
            .header(header::ACCEPT, "application/json, text/plain, */*")
            .header(header::ORIGIN, origin)
            .header(header::REFERER, &referer)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("CP-LANG", "en")
            .header("CP-MODE", "desktop")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(GymSniperError::Api(format!(
                "Booking failed ({}): {}",
                status, body
            )));
        }

        let book_response: BookClassResponse = response.json().await?;

        let ticket = book_response
            .tickets
            .into_iter()
            .next()
            .ok_or_else(|| GymSniperError::Api("No ticket in booking response".to_string()))?;

        let start_time = NaiveDateTime::parse_from_str(&ticket.start_time, "%Y-%m-%dT%H:%M:%S")
            .map_err(|e| GymSniperError::Api(format!("Failed to parse time: {}", e)))?
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| GymSniperError::Api("Invalid timezone".to_string()))?;

        Ok(BookingResult {
            name: ticket.name,
            start_time,
            trainer: ticket.trainer,
        })
    }

    pub async fn get_class_details(&self, class_id: u64) -> Result<MyBooking> {
        let url = format!(
            "{}/Classes/ClassCalendar/Details?classId={}",
            self.config.gym.base_url, class_id
        );

        let token = self.token.read().await;
        let token = token
            .as_ref()
            .ok_or_else(|| GymSniperError::Auth("Not logged in".to_string()))?;

        let origin = &self.config.gym.base_url.replace("/clientportal2", "");
        let referer = format!("{}/", self.config.gym.base_url);

        let response = self
            .client
            .get(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .header(header::ACCEPT, "application/json, text/plain, */*")
            .header(header::ORIGIN, origin)
            .header(header::REFERER, &referer)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("CP-LANG", "en")
            .header("CP-MODE", "desktop")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(GymSniperError::Api(format!(
                "Failed to get class details: {}",
                response.status()
            )));
        }

        let details: ClassDetailsResponse = response.json().await?;

        let start_time = NaiveDateTime::parse_from_str(&details.start_time, "%Y-%m-%dT%H:%M:%S")
            .map_err(|e| GymSniperError::Api(format!("Failed to parse time: {}", e)))?
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| GymSniperError::Api("Invalid timezone".to_string()))?;

        // Find current user's waitlist position
        let waitlist_position = details
            .users
            .iter()
            .find(|u| u.user.is_current_user)
            .and_then(|u| u.standby_queue_number);

        Ok(MyBooking {
            id: details.id,
            name: details.name,
            start_time,
            status: details.status,
            waitlist_position,
            trainer: details.trainer,
        })
    }

    pub async fn get_my_bookings(&self) -> Result<Vec<MyBooking>> {
        let classes = self.get_weekly_classes(14).await?;
        let mut bookings = Vec::new();

        for class in classes {
            if class.status == "Booked" || class.status == "Awaiting" {
                match self.get_class_details(class.id).await {
                    Ok(booking) => bookings.push(booking),
                    Err(_) => continue,
                }
            }
        }

        Ok(bookings)
    }
}

fn parse_class_item(item: ClassItem) -> Result<ClassInfo> {
    let start_time = NaiveDateTime::parse_from_str(&item.start_time, "%Y-%m-%dT%H:%M:%S")
        .map_err(|e| GymSniperError::Api(format!("Failed to parse start time: {}", e)))?
        .and_local_timezone(Local)
        .single()
        .ok_or_else(|| GymSniperError::Api("Invalid timezone for start".to_string()))?;

    Ok(ClassInfo {
        id: item.id,
        name: item.name,
        start_time,
        status: item.status,
        trainer: item.trainer,
    })
}
