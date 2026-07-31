variable "location" {
  description = "Azure region for all resources."
  type        = string
  default     = "eastus2"
}

variable "postgres_admin_password" {
  description = "Postgres admin password. Set via TF_VAR_postgres_admin_password — never in a file."
  type        = string
  sensitive   = true
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
