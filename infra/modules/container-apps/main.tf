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

resource "azurerm_user_assigned_identity" "api" {
  name                = "${var.name_prefix}-api-identity"
  resource_group_name = var.resource_group_name
  location            = var.location

  tags = var.tags
}

resource "azurerm_role_assignment" "api_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.api.principal_id
}

resource "azurerm_container_app" "api" {
  name                         = "${var.name_prefix}-api"
  resource_group_name          = var.resource_group_name
  container_app_environment_id = azurerm_container_app_environment.main.id
  revision_mode                = "Single"

  identity {
    type         = "UserAssigned"
    identity_ids = [azurerm_user_assigned_identity.api.id]
  }

  registry {
    server   = var.registry_login_server
    identity = azurerm_user_assigned_identity.api.id
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
      name   = "api"
      image  = "${var.registry_login_server}/summa-api:${var.image_tag}"
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

  depends_on = [azurerm_role_assignment.api_acr_pull]
}

resource "azurerm_user_assigned_identity" "worker" {
  name                = "${var.name_prefix}-worker-identity"
  resource_group_name = var.resource_group_name
  location            = var.location

  tags = var.tags
}

resource "azurerm_role_assignment" "worker_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.worker.principal_id
}

resource "azurerm_container_app" "worker" {
  name                         = "${var.name_prefix}-worker"
  resource_group_name          = var.resource_group_name
  container_app_environment_id = azurerm_container_app_environment.main.id
  revision_mode                = "Single"

  identity {
    type         = "UserAssigned"
    identity_ids = [azurerm_user_assigned_identity.worker.id]
  }

  registry {
    server   = var.registry_login_server
    identity = azurerm_user_assigned_identity.worker.id
  }

  secret {
    name  = "connection-string"
    value = var.connection_string
  }

  template {
    min_replicas = 1
    max_replicas = 1

    container {
      name   = "worker"
      image  = "${var.registry_login_server}/summa-projections:${var.image_tag}"
      cpu    = 0.25
      memory = "0.5Gi"

      env {
        name        = "ConnectionStrings__Summa"
        secret_name = "connection-string"
      }
    }
  }

  tags = var.tags

  depends_on = [azurerm_role_assignment.worker_acr_pull]
}

resource "azurerm_user_assigned_identity" "migrate" {
  name                = "${var.name_prefix}-migrate-identity"
  resource_group_name = var.resource_group_name
  location            = var.location

  tags = var.tags
}

resource "azurerm_role_assignment" "migrate_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.migrate.principal_id
}

resource "azurerm_container_app_job" "migrate" {
  name                         = "${var.name_prefix}-migrate"
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
    identity_ids = [azurerm_user_assigned_identity.migrate.id]
  }

  registry {
    server   = var.registry_login_server
    identity = azurerm_user_assigned_identity.migrate.id
  }

  secret {
    name  = "postgres-password"
    value = var.postgres_admin_password
  }

  template {
    container {
      name   = "migrate"
      image  = "${var.registry_login_server}/summa-migrate:${var.image_tag}"
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

  depends_on = [azurerm_role_assignment.migrate_acr_pull]
}
