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

resource "azurerm_container_app" "api" {
  name                         = "${var.name_prefix}-api"
  resource_group_name          = var.resource_group_name
  container_app_environment_id = azurerm_container_app_environment.main.id
  revision_mode                = "Single"

  identity {
    type = "SystemAssigned"
  }

  registry {
    server   = var.registry_login_server
    identity = "system"
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

      liveness_probe {
        transport = "HTTP"
        path      = "/health"
        port      = 8080
      }
    }
  }

  tags = var.tags
}

resource "azurerm_container_app" "worker" {
  name                         = "${var.name_prefix}-worker"
  resource_group_name          = var.resource_group_name
  container_app_environment_id = azurerm_container_app_environment.main.id
  revision_mode                = "Single"

  identity {
    type = "SystemAssigned"
  }

  registry {
    server   = var.registry_login_server
    identity = "system"
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
}

resource "azurerm_role_assignment" "api_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_container_app.api.identity[0].principal_id
}

resource "azurerm_role_assignment" "worker_acr_pull" {
  scope                = var.registry_id
  role_definition_name = "AcrPull"
  principal_id         = azurerm_container_app.worker.identity[0].principal_id
}
