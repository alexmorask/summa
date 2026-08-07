variable "image_tag" {
  description = "Tag to pull for both the API and worker Container App images — the commit SHA being deployed. No default: every apply (local bootstrap or CD) must pass one explicitly, so a stale or floating tag is never applied by accident."
  type        = string
}
