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

variable "location" {
  description = "Azure region for all resources."
  type        = string
  default     = "eastus2"
}
