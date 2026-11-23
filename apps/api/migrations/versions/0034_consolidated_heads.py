"""Reconcile persisted model, collaboration and prior unified migration heads."""

revision = "0034_consolidated_heads"
down_revision = ("0030_model_vault", "0033_feature_comments", "0032_unified_collaboration")
branch_labels = None
depends_on = None


def upgrade() -> None:
    pass


def downgrade() -> None:
    pass
