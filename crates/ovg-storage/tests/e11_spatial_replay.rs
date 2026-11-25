use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::e11_outbox::{SpatialCursor, SpatialReplay};
use ovg_storage::{DbPool, DbPoolConfig};
use uuid::Uuid;

fn fixture(key: &str) -> Uuid {
    std::env::var(key).unwrap().parse().unwrap()
}

#[tokio::test]
#[ignore = "requires isolated migrated PostGIS fixture; executed by E11 CI"]
async fn postgres_replay_is_bounded_and_authorized() {
    let pool = DbPool::connect(
        &std::env::var("E11_RUNTIME_URL").unwrap(),
        DbPoolConfig::default(),
    )
    .await
    .unwrap();
    let org = fixture("E11_ORG");
    let project = fixture("E11_PROJECT");
    let user = fixture("E11_USER");
    let session = fixture("E11_SESSION");
    let mut read = ProjectRead::begin(
        &pool,
        org,
        project,
        user,
        session,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .unwrap();

    let cursor = match read.replay_spatial_invalidations(None, 1).await.unwrap() {
        SpatialReplay::SnapshotRequired {
            latest_cursor: Some(cursor),
        } => cursor,
        other => panic!("expected retention fallback, got {other:?}"),
    };
    let start = SpatialCursor {
        epoch: cursor.epoch,
        sequence: cursor.sequence - 2,
    };
    match read
        .replay_spatial_invalidations(Some(start), 1)
        .await
        .unwrap()
    {
        SpatialReplay::Events {
            events,
            next_cursor: Some(next),
            has_more,
        } => {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].sequence, start.sequence + 1);
            assert_eq!(events[0].kind, "spatial_change");
            assert_eq!(next.sequence, start.sequence + 1);
            assert!(has_more);
        }
        other => panic!("expected page, got {other:?}"),
    }
    assert!(matches!(
        read.replay_spatial_invalidations(
            Some(SpatialCursor {
                epoch: Uuid::new_v4(),
                sequence: 1,
            }),
            10,
        )
        .await
        .unwrap(),
        SpatialReplay::SnapshotRequired { .. }
    ));
    assert!(read.replay_spatial_invalidations(None, 501).await.is_err());
    read.commit().await.unwrap();

    assert!(ProjectRead::begin(
        &pool,
        fixture("E11_OTHER_ORG"),
        fixture("E11_OTHER_PROJECT"),
        user,
        session,
        ProjectReadPolicy::Unrestricted,
    )
    .await
    .is_err());
}
