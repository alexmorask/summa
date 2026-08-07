variable "location" {
  description = "Azure region for all resources."
  type        = string
  default     = "eastus2"
}

variable "admin_ip_address" {
  description = "Your current public IP, allowlisted on the Postgres firewall for migrations/admin access. Look it up yourself (e.g. `curl https://api.ipify.org`) and pass it explicitly via -var or TF_VAR_admin_ip_address — deliberately not fetched automatically by Terraform, since piping an unauthenticated third-party service's response straight into a database firewall rule with no human review is a real supply-chain risk, not just a convenience trade-off."
  type        = string
}

variable "budget_alert_email" {
  description = "Email address for budget alert notifications."
  type        = string
  default     = "alexmorask@gmail.com"
}

variable "budget_amount" {
  description = "Monthly budget amount (USD) that triggers alert notifications. Set above the ~$21/month expected baseline (Postgres B1ms + 32GB storage, ACR Basic) so the alert signals a real overage, not routine spend."
  type        = number
  default     = 30
}

variable "image_tag" {
  description = "Tag to pull for both the API and worker Container App images — the commit SHA being deployed. No default: every apply (local bootstrap or CD) must pass one explicitly, so a stale or floating tag is never applied by accident."
  type        = string
}
