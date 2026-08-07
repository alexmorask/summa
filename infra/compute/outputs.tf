output "api_url" {
  description = "Public URL of the deployed API."
  value       = "https://${module.container_apps.api_fqdn}"
}
