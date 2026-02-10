use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use gym_sniper::api::PerfectGymClient;
use gym_sniper::config::{Config, Credentials, GymConfig};

/// Create a test config pointed at the mock server
fn test_config(base_url: &str) -> Config {
    Config {
        gym: GymConfig {
            base_url: base_url.to_string(),
            club_id: 1,
        },
        credentials: Credentials {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
        },
        targets: vec![],
        email: None,
    }
}

/// Mount a successful login mock that returns a JWT token
async fn mount_login(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/Auth/Login"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("jwt-token", "test-jwt-token-123")
                .set_body_json(serde_json::json!({
                    "User": {
                        "Member": {
                            "Id": 42,
                            "FirstName": "Test"
                        }
                    }
                })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

// ── Login tests ──────────────────────────────────────────────────

#[tokio::test]
async fn login_success() {
    let server = MockServer::start().await;
    mount_login(&server).await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    let result = client.login().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn login_failure_401() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/Auth/Login"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    let result = client.login().await;
    assert!(result.is_err());
    let err = match result {
        Err(e) => format!("{}", e),
        Ok(_) => panic!("Expected error"),
    };
    assert!(err.contains("Authentication"), "Expected auth error, got: {}", err);
}

// ── get_weekly_classes tests ─────────────────────────────────────

#[tokio::test]
async fn get_weekly_classes_parses_and_sorts() {
    let server = MockServer::start().await;
    mount_login(&server).await;

    Mock::given(method("POST"))
        .and(path("/Classes/ClassCalendar/WeeklyClasses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "CalendarData": [
                {
                    "ZoneName": "Studio A",
                    "ClassesPerHour": [
                        {
                            "ClassesPerDay": [
                                [
                                    {
                                        "Id": 2,
                                        "Name": "Spin",
                                        "StartTime": "2025-01-15T18:00:00",
                                        "Duration": "45",
                                        "Status": "Bookable",
                                        "Trainer": "Bob"
                                    }
                                ],
                                [
                                    {
                                        "Id": 1,
                                        "Name": "Yoga",
                                        "StartTime": "2025-01-15T09:00:00",
                                        "Duration": "60",
                                        "Status": "Full",
                                        "Trainer": null
                                    }
                                ]
                            ]
                        }
                    ]
                }
            ]
        })))
        .mount(&server)
        .await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    client.login().await.unwrap();
    let classes = client.get_weekly_classes(7).await.unwrap();

    assert_eq!(classes.len(), 2);
    // Should be sorted by start_time, so Yoga (09:00) first
    assert_eq!(classes[0].name, "Yoga");
    assert_eq!(classes[0].id, 1);
    assert_eq!(classes[0].status, "Full");
    assert_eq!(classes[0].trainer, None);

    assert_eq!(classes[1].name, "Spin");
    assert_eq!(classes[1].id, 2);
    assert_eq!(classes[1].trainer, Some("Bob".to_string()));
}

#[tokio::test]
async fn get_weekly_classes_empty_response() {
    let server = MockServer::start().await;
    mount_login(&server).await;

    Mock::given(method("POST"))
        .and(path("/Classes/ClassCalendar/WeeklyClasses"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "CalendarData": []
            })),
        )
        .mount(&server)
        .await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    client.login().await.unwrap();
    let classes = client.get_weekly_classes(7).await.unwrap();
    assert!(classes.is_empty());
}

// ── book_class tests ─────────────────────────────────────────────

#[tokio::test]
async fn book_class_success() {
    let server = MockServer::start().await;
    mount_login(&server).await;

    Mock::given(method("POST"))
        .and(path("/Classes/ClassCalendar/BookClass"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Tickets": [
                {
                    "Name": "Morning Yoga",
                    "StartTime": "2025-01-20T09:00:00",
                    "Trainer": "Alice"
                }
            ],
            "ClassId": 555
        })))
        .mount(&server)
        .await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    client.login().await.unwrap();
    let result = client.book_class(555).await.unwrap();

    assert_eq!(result.name, "Morning Yoga");
    assert_eq!(result.trainer, Some("Alice".to_string()));
    assert_eq!(
        result.start_time.format("%Y-%m-%d %H:%M").to_string(),
        "2025-01-20 09:00"
    );
}

#[tokio::test]
async fn book_class_failure_400() {
    let server = MockServer::start().await;
    mount_login(&server).await;

    Mock::given(method("POST"))
        .and(path("/Classes/ClassCalendar/BookClass"))
        .respond_with(ResponseTemplate::new(400).set_body_string("TooSoonToBook"))
        .mount(&server)
        .await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    client.login().await.unwrap();
    let result = client.book_class(555).await;
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("400") || err.contains("Booking failed"), "Got: {}", err);
}

// ── get_class_details tests ──────────────────────────────────────

#[tokio::test]
async fn get_class_details_with_trainer_and_waitlist() {
    let server = MockServer::start().await;
    mount_login(&server).await;

    Mock::given(method("GET"))
        .and(path("/Classes/ClassCalendar/Details"))
        .and(query_param("classId", "123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Id": 123,
            "Name": "HIIT",
            "Status": "Awaiting",
            "StartTime": "2025-02-01T10:30:00",
            "TrainerDetails": { "Title": "Coach Mike" },
            "Users": [
                {
                    "Status": "Awaiting",
                    "StandByQueueNumber": 3,
                    "User": { "IsCurrentUser": true }
                },
                {
                    "Status": "Booked",
                    "StandByQueueNumber": null,
                    "User": { "IsCurrentUser": false }
                }
            ]
        })))
        .mount(&server)
        .await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    client.login().await.unwrap();
    let booking = client.get_class_details(123).await.unwrap();

    assert_eq!(booking.id, 123);
    assert_eq!(booking.name, "HIIT");
    assert_eq!(booking.trainer, Some("Coach Mike".to_string()));
    assert_eq!(booking.waitlist_position, Some(3));
}

// ── cancel_booking tests ─────────────────────────────────────────

#[tokio::test]
async fn cancel_booking_success() {
    let server = MockServer::start().await;
    mount_login(&server).await;

    Mock::given(method("POST"))
        .and(path("/Classes/ClassCalendar/CancelBooking"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    client.login().await.unwrap();
    let result = client.cancel_booking(999).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn cancel_booking_failure() {
    let server = MockServer::start().await;
    mount_login(&server).await;

    Mock::given(method("POST"))
        .and(path("/Classes/ClassCalendar/CancelBooking"))
        .respond_with(ResponseTemplate::new(400).set_body_string("Cannot cancel"))
        .mount(&server)
        .await;

    let config = test_config(&server.uri());
    let client = PerfectGymClient::new(&config);
    client.login().await.unwrap();
    let result = client.cancel_booking(999).await;
    assert!(result.is_err());
}
