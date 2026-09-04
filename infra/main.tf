# ---------------------------------------------------------------------------
# Tinkhaven Typing — container on ECS Fargate behind an ALB.
#
# Shape and cost notes, since both were deliberate choices:
#
#   * Tasks run in the default VPC's public subnets with public IPs, rather than
#     private subnets. Private subnets would need a NAT gateway to reach ECR and
#     DynamoDB, and a NAT gateway costs more per month than everything else here
#     combined. The task's security group accepts traffic only from the load
#     balancer, so it is not reachable from the internet regardless.
#   * The load balancer is the one substantial fixed cost, roughly $18/month.
#     The rest — 0.25 vCPU of Fargate, on-demand DynamoDB, ECR, Route 53 — comes
#     to about $10/month at this size.
#   * State is a leaderboard: a few writes a day. DynamoDB on-demand suits that
#     far better than running a database for it.
# ---------------------------------------------------------------------------

locals {
  tags = {
    Project   = var.project
    ManagedBy = "terraform"
  }
}

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = local.tags
  }
}

data "aws_caller_identity" "current" {}

# ---------------------------------------------------------------------------
# Network: the account's default VPC and its subnets
# ---------------------------------------------------------------------------

data "aws_vpc" "default" {
  default = true
}

# One subnet per availability zone, and exactly one.
#
# A load balancer refuses to attach to two subnets in the same AZ, and a default
# VPC can easily have more than one there — an extra subnet added by hand at some
# point is enough. `default-for-az` is the subnet AWS created with the VPC, so
# this yields precisely one per zone without hard-coding any ids.
data "aws_subnets" "default" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.default.id]
  }

  filter {
    name   = "default-for-az"
    values = ["true"]
  }
}

resource "aws_security_group" "alb" {
  name        = "${var.project}-alb"
  description = "Public HTTP/HTTPS ingress for ${var.domain_name}"
  vpc_id      = data.aws_vpc.default.id

  ingress {
    description = "HTTPS from anywhere"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  ingress {
    description = "HTTP from anywhere, redirected to HTTPS"
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  egress {
    description = "To the tasks"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_security_group" "task" {
  name        = "${var.project}-task"
  description = "Application container; reachable only from the load balancer"
  vpc_id      = data.aws_vpc.default.id

  ingress {
    description     = "From the load balancer only"
    from_port       = 8080
    to_port         = 8080
    protocol        = "tcp"
    security_groups = [aws_security_group.alb.id]
  }

  egress {
    description = "Outbound for ECR, DynamoDB and CloudWatch"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

# ---------------------------------------------------------------------------
# Image registry
# ---------------------------------------------------------------------------

resource "aws_ecr_repository" "app" {
  name                 = var.project
  image_tag_mutability = "MUTABLE"
  force_delete         = true

  image_scanning_configuration {
    scan_on_push = true
  }
}

resource "aws_ecr_lifecycle_policy" "app" {
  repository = aws_ecr_repository.app.name

  policy = jsonencode({
    rules = [{
      rulePriority = 1
      description  = "Keep the ten most recent images"
      selection = {
        tagStatus   = "any"
        countType   = "imageCountMoreThan"
        countNumber = 10
      }
      action = { type = "expire" }
    }]
  })
}

# ---------------------------------------------------------------------------
# Leaderboard table
#
# One table, partitioned by "<module>#<language>". The sort key is a zero-padded
# complement of the speed, so a plain forward query returns the fastest first
# without a secondary index — see sort_key() in server/leaderboard.rs.
# ---------------------------------------------------------------------------

resource "aws_dynamodb_table" "leaderboard" {
  name         = "${var.project}-leaderboard"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "board"
  range_key    = "result"

  attribute {
    name = "board"
    type = "S"
  }

  attribute {
    name = "result"
    type = "S"
  }

  # Published rows expire, so pseudonymous entries do not pile up forever.
  ttl {
    attribute_name = "expires_at"
    enabled        = true
  }

  point_in_time_recovery {
    enabled = true
  }
}

# ---------------------------------------------------------------------------
# Signed-in visitors' progress
#
# One item per user, keyed by the pseudonymous identifier derived in
# server/auth.rs. Holds no email address, no name and no provider identifier.
# ---------------------------------------------------------------------------

resource "aws_dynamodb_table" "profiles" {
  name         = "${var.project}-profiles"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "user_id"

  attribute {
    name = "user_id"
    type = "S"
  }

  # Pushed forward on every write, so only abandoned profiles expire.
  ttl {
    attribute_name = "expires_at"
    enabled        = true
  }

  point_in_time_recovery {
    enabled = true
  }
}

# ---------------------------------------------------------------------------
# Logs
# ---------------------------------------------------------------------------

resource "aws_cloudwatch_log_group" "app" {
  name              = "/ecs/${var.project}"
  retention_in_days = var.log_retention_days
}

# ---------------------------------------------------------------------------
# IAM
# ---------------------------------------------------------------------------

data "aws_iam_policy_document" "ecs_assume_role" {
  statement {
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["ecs-tasks.amazonaws.com"]
    }
  }
}

# Used by the ECS agent to pull the image and ship logs.
resource "aws_iam_role" "execution" {
  name               = "${var.project}-execution"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume_role.json
}

resource "aws_iam_role_policy_attachment" "execution" {
  role       = aws_iam_role.execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

# Used by the application itself: the leaderboard table, nothing else.
resource "aws_iam_role" "task" {
  name               = "${var.project}-task"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume_role.json
}

data "aws_iam_policy_document" "leaderboard_access" {
  statement {
    sid       = "LeaderboardReadWrite"
    actions   = ["dynamodb:Query", "dynamodb:PutItem"]
    resources = [aws_dynamodb_table.leaderboard.arn]
  }

  statement {
    sid = "ProfileReadWrite"
    # Delete included so a signed-in visitor can erase their own profile from
    # the app rather than by emailing the operator.
    actions   = ["dynamodb:GetItem", "dynamodb:PutItem", "dynamodb:DeleteItem"]
    resources = [aws_dynamodb_table.profiles.arn]
  }
}

# ---------------------------------------------------------------------------
# Sign-in secrets
#
# The parameters are created out of band; Terraform references them by ARN and
# never reads a value, so nothing secret reaches the state file. The *execution*
# role needs the read permission, not the task role: the ECS agent resolves a
# task definition's `secrets` before the container starts.
# ---------------------------------------------------------------------------

locals {
  ssm_base = "arn:aws:ssm:${var.aws_region}:${data.aws_caller_identity.current.account_id}:parameter${var.ssm_prefix}"

  sign_in_secret_arns = [
    "${local.ssm_base}/google_client_id",
    "${local.ssm_base}/google_client_secret",
    "${local.ssm_base}/session_secret",
  ]

  sign_in_secrets = var.enable_sign_in ? [
    { name = "GOOGLE_CLIENT_ID", valueFrom = "${local.ssm_base}/google_client_id" },
    { name = "GOOGLE_CLIENT_SECRET", valueFrom = "${local.ssm_base}/google_client_secret" },
    { name = "SESSION_SECRET", valueFrom = "${local.ssm_base}/session_secret" },
  ] : []
}

data "aws_iam_policy_document" "read_sign_in_secrets" {
  statement {
    sid       = "ReadSignInParameters"
    actions   = ["ssm:GetParameters"]
    resources = local.sign_in_secret_arns
  }

  statement {
    sid     = "DecryptSecureStrings"
    actions = ["kms:Decrypt"]
    # The AWS-managed key SSM uses for SecureString parameters.
    resources = ["arn:aws:kms:${var.aws_region}:${data.aws_caller_identity.current.account_id}:alias/aws/ssm"]
  }
}

resource "aws_iam_role_policy" "read_sign_in_secrets" {
  count  = var.enable_sign_in ? 1 : 0
  name   = "${var.project}-sign-in-secrets"
  role   = aws_iam_role.execution.id
  policy = data.aws_iam_policy_document.read_sign_in_secrets.json
}

resource "aws_iam_role_policy" "leaderboard_access" {
  name   = "${var.project}-leaderboard"
  role   = aws_iam_role.task.id
  policy = data.aws_iam_policy_document.leaderboard_access.json
}

# ---------------------------------------------------------------------------
# Certificate and DNS
# ---------------------------------------------------------------------------

resource "aws_acm_certificate" "cert" {
  domain_name       = var.domain_name
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}

data "aws_route53_zone" "zone" {
  name         = var.route53_zone_name
  private_zone = false
}

resource "aws_route53_record" "cert_validation" {
  for_each = {
    for option in aws_acm_certificate.cert.domain_validation_options :
    option.domain_name => {
      name   = option.resource_record_name
      record = option.resource_record_value
      type   = option.resource_record_type
    }
  }

  zone_id         = data.aws_route53_zone.zone.zone_id
  name            = each.value.name
  type            = each.value.type
  records         = [each.value.record]
  ttl             = 60
  allow_overwrite = true
}

resource "aws_acm_certificate_validation" "cert" {
  certificate_arn         = aws_acm_certificate.cert.arn
  validation_record_fqdns = [for record in aws_route53_record.cert_validation : record.fqdn]
}

resource "aws_route53_record" "site" {
  zone_id = data.aws_route53_zone.zone.zone_id
  name    = var.domain_name
  type    = "A"

  alias {
    name                   = aws_lb.app.dns_name
    zone_id                = aws_lb.app.zone_id
    evaluate_target_health = true
  }
}

# ---------------------------------------------------------------------------
# Load balancer
# ---------------------------------------------------------------------------

resource "aws_lb" "app" {
  name               = var.project
  load_balancer_type = "application"
  subnets            = data.aws_subnets.default.ids
  security_groups    = [aws_security_group.alb.id]

  # WebSockets: the practice socket is quiet between exercises, and the default
  # 60s idle timeout would close it repeatedly. The client also pings every 30s
  # (PING_INTERVAL_MS), so this only has to be comfortably longer than that.
  idle_timeout = 300

  drop_invalid_header_fields = true
}

resource "aws_lb_target_group" "app" {
  name        = var.project
  port        = 8080
  protocol    = "HTTP"
  target_type = "ip"
  vpc_id      = data.aws_vpc.default.id

  health_check {
    path                = "/health"
    matcher             = "200"
    interval            = 30
    timeout             = 5
    healthy_threshold   = 2
    unhealthy_threshold = 3
  }

  # Let in-flight WebSocket connections finish rather than cutting them at the
  # 300s default during a deployment.
  deregistration_delay = 30
}

resource "aws_lb_listener" "https" {
  load_balancer_arn = aws_lb.app.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = aws_acm_certificate_validation.cert.certificate_arn

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.app.arn
  }
}

resource "aws_lb_listener" "http_redirect" {
  load_balancer_arn = aws_lb.app.arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type = "redirect"

    redirect {
      port        = "443"
      protocol    = "HTTPS"
      status_code = "HTTP_301"
    }
  }
}

# ---------------------------------------------------------------------------
# ECS
# ---------------------------------------------------------------------------

resource "aws_ecs_cluster" "app" {
  name = var.project

  setting {
    name  = "containerInsights"
    value = "disabled" # extra cost, and CloudWatch logs cover this size of app
  }
}

resource "aws_ecs_task_definition" "app" {
  family                   = var.project
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = var.task_cpu
  memory                   = var.task_memory
  execution_role_arn       = aws_iam_role.execution.arn
  task_role_arn            = aws_iam_role.task.arn

  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = var.cpu_architecture
  }

  container_definitions = jsonencode([{
    name      = "app"
    image     = "${aws_ecr_repository.app.repository_url}:latest"
    essential = true

    portMappings = [{
      containerPort = 8080
      protocol      = "tcp"
    }]

    environment = [
      { name = "LEADERBOARD_TABLE", value = aws_dynamodb_table.leaderboard.name },
      { name = "PROFILES_TABLE", value = aws_dynamodb_table.profiles.name },
      { name = "AWS_REGION", value = var.aws_region },
      { name = "RUST_LOG", value = "info,typing_web=info" },
      # The OAuth redirect must match what is registered with Google exactly,
      # so it is derived from the public hostname rather than guessed.
      { name = "PUBLIC_BASE_URL", value = "https://${var.domain_name}" },
    ]

    # Empty until enable_sign_in is set, which keeps the task startable before
    # the parameters exist.
    secrets = local.sign_in_secrets

    logConfiguration = {
      logDriver = "awslogs"
      options = {
        "awslogs-group"         = aws_cloudwatch_log_group.app.name
        "awslogs-region"        = var.aws_region
        "awslogs-stream-prefix" = "app"
      }
    }

    healthCheck = {
      command     = ["CMD", "/usr/local/bin/typing-web", "--health-check"]
      interval    = 30
      timeout     = 5
      retries     = 3
      startPeriod = 10
    }
  }])
}

resource "aws_ecs_service" "app" {
  name            = var.project
  cluster         = aws_ecs_cluster.app.id
  task_definition = aws_ecs_task_definition.app.arn
  desired_count   = var.desired_count
  launch_type     = "FARGATE"

  # Start the replacement before stopping the old task, so a deploy does not
  # take the site down while only one task is running.
  deployment_minimum_healthy_percent = 100
  deployment_maximum_percent         = 200

  network_configuration {
    subnets = data.aws_subnets.default.ids
    # Public IP so the task can reach ECR and DynamoDB without a NAT gateway.
    # Inbound is still restricted to the load balancer by the security group.
    assign_public_ip = true
    security_groups  = [aws_security_group.task.id]
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.app.arn
    container_name   = "app"
    container_port   = 8080
  }

  # Give the container time to start before the first health check counts.
  health_check_grace_period_seconds = 30

  # deploy.sh pushes a new :latest and forces a new deployment, so the task
  # definition revision recorded here will drift. That is expected.
  lifecycle {
    ignore_changes = [task_definition]
  }

  depends_on = [aws_lb_listener.https]
}
