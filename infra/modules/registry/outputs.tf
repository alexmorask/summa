output "id" {
  description = "Container Registry resource ID."
  value       = azurerm_container_registry.main.id
}

output "login_server" {
  description = "Registry login server, e.g. summacrXXXXXXXX.azurecr.io."
  value       = azurerm_container_registry.main.login_server
}

output "name" {
  description = "Container Registry name."
  value       = azurerm_container_registry.main.name
}
