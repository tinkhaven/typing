output "site_url" {
  description = "Where the site will be served."
  value       = "https://${var.domain_name}"
}

output "ecr_repository_url" {
  description = "Push target for deploy.sh."
  value       = aws_ecr_repository.app.repository_url
}

output "ecs_cluster" {
  description = "Cluster name, for deploy.sh and the AWS console."
  value       = aws_ecs_cluster.app.name
}

output "ecs_service" {
  description = "Service name, for forcing a new deployment."
  value       = aws_ecs_service.app.name
}

output "leaderboard_table" {
  description = "DynamoDB table holding published results."
  value       = aws_dynamodb_table.leaderboard.name
}

output "log_group" {
  description = "Where container logs go."
  value       = aws_cloudwatch_log_group.app.name
}

output "alb_dns_name" {
  description = "Load balancer hostname, useful before DNS propagates."
  value       = aws_lb.app.dns_name
}

output "aws_region" {
  description = "Region everything lives in; read by deploy.sh."
  value       = var.aws_region
}

output "profiles_table" {
  description = "DynamoDB table holding signed-in visitors' progress."
  value       = aws_dynamodb_table.profiles.name
}

output "google_redirect_uri" {
  description = "Register exactly this as the authorised redirect URI in Google Cloud Console."
  value       = "https://${var.domain_name}/auth/google/callback"
}
