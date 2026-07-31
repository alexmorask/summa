resource "azurerm_postgresql_flexible_server" "main" {
  name                = var.name
  resource_group_name = var.resource_group_name
  location            = var.location

  version = "17"

  administrator_login    = var.administrator_login
  administrator_password = var.administrator_password

  storage_mb = 32768
  sku_name   = "B_Standard_B1ms"

  # No VNet/delegated subnet in this slice yet, so the API/worker (Stage 13)
  # and an admin machine both need to reach this over the public endpoint.
  # Access is still gated by the firewall rules below, not left wide open.
  public_network_access_enabled = true

  tags = var.tags
}

resource "azurerm_postgresql_flexible_server_database" "summa" {
  name      = "summa"
  server_id = azurerm_postgresql_flexible_server.main.id
}

# The "AllowAzureServices" rule (0.0.0.0/0.0.0.0 — Azure's special-case
# shorthand for "any Azure-hosted service, any subscription") is
# deliberately not created here. Adding it now would widen this server's
# exposed surface for zero benefit, since nothing in Azure needs to reach
# it until Stage 13's Container Apps exist. Add it as part of that stage,
# when there's a real consumer.

resource "azurerm_postgresql_flexible_server_firewall_rule" "allow_admin_ip" {
  name             = "AllowAdminIp"
  server_id        = azurerm_postgresql_flexible_server.main.id
  start_ip_address = var.admin_ip_address
  end_ip_address   = var.admin_ip_address
}
