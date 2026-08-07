resource "azurerm_log_analytics_workspace" "main" {
  name                = "${var.name_prefix}-logs"
  resource_group_name = var.resource_group_name
  location            = var.location

  sku               = "PerGB2018"
  retention_in_days = 30

  tags = var.tags
}

resource "azurerm_container_app_environment" "main" {
  name                       = "${var.name_prefix}-env"
  resource_group_name        = var.resource_group_name
  location                   = var.location
  log_analytics_workspace_id = azurerm_log_analytics_workspace.main.id

  tags = var.tags
}

resource "azurerm_user_assigned_identity" "ledger_api" {
  name                = "ledger-api-identity"
  resource_group_name = var.resource_group_name
  location            = var.location

  tags = var.tags
}

resource "azurerm_role_assignment" "ledger_api_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.ledger_api.principal_id
  principal_type       = "ServicePrincipal"
}

resource "azurerm_container_app" "ledger_api" {
  name                         = "ledger-api"
  resource_group_name          = var.resource_group_name
  container_app_environment_id = azurerm_container_app_environment.main.id
  revision_mode                = "Single"

  identity {
    type         = "UserAssigned"
    identity_ids = [azurerm_user_assigned_identity.ledger_api.id]
  }

  registry {
    server   = var.registry_login_server
    identity = azurerm_user_assigned_identity.ledger_api.id
  }

  secret {
    name  = "connection-string"
    value = var.connection_string
  }

  ingress {
    external_enabled = true
    target_port      = 8080

    traffic_weight {
      latest_revision = true
      percentage      = 100
    }
  }

  template {
    min_replicas = 0
    max_replicas = 1

    container {
      name   = "ledger-api"
      image  = "${var.registry_login_server}/ledger-api:${var.image_tag}"
      cpu    = 0.25
      memory = "0.5Gi"

      env {
        name        = "ConnectionStrings__Summa"
        secret_name = "connection-string"
      }

      env {
        name  = "Authentication__Authority"
        value = var.auth_authority
      }

      env {
        name  = "Authentication__Audience"
        value = var.auth_audience
      }

      liveness_probe {
        transport = "HTTP"
        path      = "/health"
        port      = 8080
      }
    }
  }

  tags = var.tags

  depends_on = [azurerm_role_assignment.ledger_api_acr_pull]
}

resource "azurerm_user_assigned_identity" "ledger_projections" {
  name                = "ledger-projections-identity"
  resource_group_name = var.resource_group_name
  location            = var.location

  tags = var.tags
}

resource "azurerm_role_assignment" "ledger_projections_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.ledger_projections.principal_id
  principal_type       = "ServicePrincipal"
}

resource "azurerm_container_app" "ledger_projections" {
  name                         = "ledger-projections"
  resource_group_name          = var.resource_group_name
  container_app_environment_id = azurerm_container_app_environment.main.id
  revision_mode                = "Single"

  identity {
    type         = "UserAssigned"
    identity_ids = [azurerm_user_assigned_identity.ledger_projections.id]
  }

  registry {
    server   = var.registry_login_server
    identity = azurerm_user_assigned_identity.ledger_projections.id
  }

  secret {
    name  = "connection-string"
    value = var.connection_string
  }

  template {
    min_replicas = 1
    max_replicas = 1

    container {
      name   = "ledger-projections"
      image  = "${var.registry_login_server}/ledger-projections:${var.image_tag}"
      cpu    = 0.25
      memory = "0.5Gi"

      env {
        name        = "ConnectionStrings__Summa"
        secret_name = "connection-string"
      }
    }
  }

  tags = var.tags

  depends_on = [azurerm_role_assignment.ledger_projections_acr_pull]
}

resource "azurerm_user_assigned_identity" "db_migrate" {
  name                = "db-migrate-identity"
  resource_group_name = var.resource_group_name
  location            = var.location

  tags = var.tags
}

resource "azurerm_role_assignment" "db_migrate_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.db_migrate.principal_id
  principal_type       = "ServicePrincipal"
}

resource "azurerm_container_app_job" "db_migrate" {
  name                         = "db-migrate"
  resource_group_name          = var.resource_group_name
  location                     = var.location
  container_app_environment_id = azurerm_container_app_environment.main.id

  replica_timeout_in_seconds = 300
  replica_retry_limit        = 0

  manual_trigger_config {
    parallelism              = 1
    replica_completion_count = 1
  }

  identity {
    type         = "UserAssigned"
    identity_ids = [azurerm_user_assigned_identity.db_migrate.id]
  }

  registry {
    server   = var.registry_login_server
    identity = azurerm_user_assigned_identity.db_migrate.id
  }

  secret {
    name  = "postgres-password"
    value = var.postgres_admin_password
  }

  template {
    container {
      name   = "db-migrate"
      image  = "${var.registry_login_server}/db-migrate:${var.image_tag}"
      cpu    = 0.25
      memory = "0.5Gi"

      env {
        name  = "PGHOST"
        value = var.postgres_host
      }

      env {
        name  = "PGPORT"
        value = "5432"
      }

      env {
        name  = "PGUSER"
        value = var.postgres_admin_login
      }

      env {
        name        = "PGPASSWORD"
        secret_name = "postgres-password"
      }

      env {
        name  = "PGDATABASE"
        value = var.postgres_database
      }

      env {
        name  = "PGSSLMODE"
        value = "verify-full"
      }

      env {
        name  = "PGSSLROOTCERT"
        value = "system"
      }
    }
  }

  tags = var.tags

  depends_on = [azurerm_role_assignment.db_migrate_acr_pull]
}

resource "azurerm_user_assigned_identity" "recognition_job" {
  name                = "recognition-job-identity"
  resource_group_name = var.resource_group_name
  location            = var.location

  tags = var.tags
}

resource "azurerm_role_assignment" "recognition_job_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.recognition_job.principal_id
  principal_type       = "ServicePrincipal"
}

resource "azurerm_container_app_job" "recognition_job" {
  name                         = "recognition-job"
  resource_group_name          = var.resource_group_name
  location                     = var.location
  container_app_environment_id = azurerm_container_app_environment.main.id

  replica_timeout_in_seconds = 300
  replica_retry_limit        = 0

  schedule_trigger_config {
    cron_expression          = "0 6 * * *"
    parallelism              = 1
    replica_completion_count = 1
  }

  identity {
    type         = "UserAssigned"
    identity_ids = [azurerm_user_assigned_identity.recognition_job.id]
  }

  registry {
    server   = var.registry_login_server
    identity = azurerm_user_assigned_identity.recognition_job.id
  }

  secret {
    name  = "connection-string"
    value = var.connection_string
  }

  template {
    container {
      name   = "recognition-job"
      image  = "${var.registry_login_server}/recognition-job:${var.image_tag}"
      cpu    = 0.25
      memory = "0.5Gi"

      env {
        name        = "ConnectionStrings__Summa"
        secret_name = "connection-string"
      }
    }
  }

  tags = var.tags

  depends_on = [azurerm_role_assignment.recognition_job_acr_pull]
}
