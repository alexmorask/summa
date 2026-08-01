locals {
  tags = {
    project = "summa"
  }
}

resource "random_id" "suffix" {
  byte_length = 4
}

resource "azurerm_resource_group" "main" {
  name     = "summa-rg"
  location = var.location

  tags = local.tags
}

# --- Remote state backend bootstrap ---
# Created in this same apply, with local state, since the backend block in
# versions.tf can't point at a storage account that doesn't exist yet. Once
# this succeeds, activate that backend block and run
# `terraform init -migrate-state`.
resource "azurerm_storage_account" "tfstate" {
  name                = "summatfstate${random_id.suffix.hex}"
  resource_group_name = azurerm_resource_group.main.name
  location            = azurerm_resource_group.main.location

  account_tier             = "Standard"
  account_replication_type = "LRS"

  # Defense in depth for a storage account that will hold Terraform state
  # (which can contain secrets in plaintext, e.g. the Postgres password).
  allow_nested_items_to_be_public = false

  # A storage account key is a broad, unscoped credential — anyone who has
  # it gets full access regardless of Azure RBAC. Disabling it means the
  # eventual `backend "azurerm" {}` block (versions.tf, currently commented
  # out) must use `use_azuread_auth = true` instead of an access key when
  # it's activated.
  shared_access_key_enabled = false

  tags = local.tags
}

resource "azurerm_storage_container" "tfstate" {
  name                  = "tfstate"
  storage_account_id    = azurerm_storage_account.tfstate.id
  container_access_type = "private"
}

module "registry" {
  source = "./modules/registry"

  name                = "summacr${random_id.suffix.hex}"
  resource_group_name = azurerm_resource_group.main.name
  location            = azurerm_resource_group.main.location
  tags                = local.tags
}

module "database" {
  source = "./modules/database"

  name                   = "summa-postgres-${random_id.suffix.hex}"
  resource_group_name    = azurerm_resource_group.main.name
  location               = azurerm_resource_group.main.location
  administrator_login    = "summaadmin"
  administrator_password = var.postgres_admin_password
  admin_ip_address       = var.admin_ip_address
  tags                   = local.tags
}

locals {
  # Depends on the Postgres admin password, so Terraform automatically
  # treats this whole value as sensitive too (redacted from plan/apply
  # output the same way the password itself is).
  postgres_connection_string = "Host=${module.database.fqdn};Port=5432;Username=summaadmin;Password=${var.postgres_admin_password};Database=${module.database.database_name};Ssl Mode=VerifyFull"
}

module "container_apps" {
  source = "./modules/container-apps"

  resource_group_name     = azurerm_resource_group.main.name
  location                = azurerm_resource_group.main.location
  registry_id             = module.registry.id
  registry_login_server   = module.registry.login_server
  connection_string       = local.postgres_connection_string
  postgres_host           = module.database.fqdn
  postgres_admin_login    = "summaadmin"
  postgres_database       = module.database.database_name
  postgres_admin_password = var.postgres_admin_password
  image_tag               = var.image_tag
  tags                    = local.tags
}

module "github_oidc" {
  source = "./modules/github-oidc"

  github_repository          = "alexmorask/summa"
  resource_group_id          = azurerm_resource_group.main.id
  tfstate_storage_account_id = azurerm_storage_account.tfstate.id
}

resource "azurerm_consumption_budget_resource_group" "main" {
  name              = "summa-budget"
  resource_group_id = azurerm_resource_group.main.id

  amount     = var.budget_amount
  time_grain = "Monthly"

  # Azure only requires this be the first of some month, as a fixed anchor
  # for the "Monthly" time_grain window — not "when the budget was created."
  # A static date avoids re-diffing on every apply the way timestamp() would.
  time_period {
    start_date = "2026-08-01T00:00:00Z"
  }

  notification {
    enabled        = true
    threshold      = 80
    operator       = "GreaterThan"
    contact_emails = [var.budget_alert_email]
  }

  notification {
    enabled        = true
    threshold      = 100
    operator       = "GreaterThan"
    contact_emails = [var.budget_alert_email]
  }
}
