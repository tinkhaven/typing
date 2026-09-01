variable "aws_region" {
  description = <<-EOT
    Region for everything, including the ACM certificate.

    Unlike CloudFront, an Application Load Balancer needs its certificate in its
    own region, so there is no us-east-1 exception here. Ireland keeps visitor
    traffic and the leaderboard inside the EU.
  EOT
  type        = string
  default     = "eu-west-1"
}

variable "domain_name" {
  description = "Public hostname for the site."
  type        = string
  default     = "typing.tinkhaven.com"
}

variable "route53_zone_name" {
  description = "Hosted zone that manages the domain."
  type        = string
  default     = "tinkhaven.com"
}

variable "project" {
  description = "Name prefix and tag applied to every resource."
  type        = string
  default     = "tinkhaven-typing"
}

variable "task_cpu" {
  description = <<-EOT
    Fargate CPU units. 256 = 0.25 vCPU.

    Server-side rendering a page of this size and relaying keystroke batches is
    not demanding; the browser does the typing loop.
  EOT
  type        = number
  default     = 256
}

variable "task_memory" {
  description = "Fargate memory in MiB. 512 is the minimum for 0.25 vCPU."
  type        = number
  default     = 512
}

variable "cpu_architecture" {
  description = <<-EOT
    ARM64 (Graviton) or X86_64.

    ARM64 costs about 20% less and builds natively on an Apple Silicon Mac, so no
    emulation is needed when pushing an image. Switch to X86_64 only if you build
    on an Intel machine and would rather not cross-compile.
  EOT
  type        = string
  default     = "ARM64"

  validation {
    condition     = contains(["ARM64", "X86_64"], var.cpu_architecture)
    error_message = "cpu_architecture must be ARM64 or X86_64."
  }
}

variable "desired_count" {
  description = <<-EOT
    Number of running tasks.

    Kept at 1 deliberately. Live leaderboard pushes are broadcast within a task,
    so with several tasks a visitor only sees pushes from the one they are
    connected to. The board itself stays correct — it is read from DynamoDB — but
    it would refresh on navigation rather than instantly. Raising this is safe;
    just know that is the trade.
  EOT
  type        = number
  default     = 1
}

variable "log_retention_days" {
  description = "How long to keep container logs."
  type        = number
  default     = 30
}

variable "leaderboard_ttl_days" {
  description = <<-EOT
    How long a published leaderboard row lives.

    Rows carry an `expires_at` timestamp and DynamoDB deletes them, so published
    nicknames do not accumulate indefinitely. Must match FLUIDNESS_MIN_CHARS's
    neighbour in the code: see ROW_TTL_SECONDS in crates/web/src/server/leaderboard.rs.
  EOT
  type        = number
  default     = 365
}
