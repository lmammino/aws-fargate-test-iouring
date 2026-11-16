import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as logs from 'aws-cdk-lib/aws-logs';
import type { Construct } from 'constructs';

export const NAME_PREFIX = 'luciano-experiment-fargate-iouring';

// Smallest Fargate size: 0.25 vCPU / 512 MiB.
const TASK_CPU = 256;
const TASK_MEMORY_MIB = 512;
const VPC_CIDR = '10.42.0.0/24';
const MAX_AZS = 2;

export interface ProbeStackProps extends cdk.StackProps {
  /** Image tag pushed by scripts/deploy.sh. */
  readonly imageTag: string;
  /** Optional existing VPC. Must have public subnets. Otherwise a minimal VPC is created. */
  readonly vpcId?: string;
}

/**
 * Everything needed to run one standalone Fargate task with RunTask:
 * ECR repo, cluster, task definition, log group, and egress-only networking.
 * No service, load balancer or NAT gateway.
 */
export class ProbeStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: ProbeStackProps) {
    super(scope, id, props);

    const repository = new ecr.Repository(this, 'Repository', {
      repositoryName: NAME_PREFIX,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
      emptyOnDelete: true,
    });

    const vpc = this.vpc(props.vpcId);

    // No ingress rules. Egress is needed for ECR pulls and CloudWatch Logs.
    const securityGroup = new ec2.SecurityGroup(this, 'TaskSecurityGroup', {
      vpc,
      securityGroupName: `${NAME_PREFIX}-task`,
      description: 'io_uring probe task: no inbound, all outbound',
      allowAllOutbound: true,
    });

    const cluster = new ecs.Cluster(this, 'Cluster', {
      clusterName: NAME_PREFIX,
      vpc,
    });

    const logGroup = new logs.LogGroup(this, 'LogGroup', {
      logGroupName: `/ecs/${NAME_PREFIX}`,
      retention: logs.RetentionDays.ONE_WEEK,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    // The execution role (ECR pull, log writes) is created by CDK.
    const taskDefinition = new ecs.FargateTaskDefinition(this, 'TaskDefinition', {
      family: NAME_PREFIX,
      cpu: TASK_CPU,
      memoryLimitMiB: TASK_MEMORY_MIB,
      runtimePlatform: {
        operatingSystemFamily: ecs.OperatingSystemFamily.LINUX,
        cpuArchitecture: ecs.CpuArchitecture.X86_64,
      },
    });

    // Default security settings on purpose: not privileged, no added capabilities.
    taskDefinition.addContainer('probe', {
      containerName: 'probe',
      image: ecs.ContainerImage.fromEcrRepository(repository, props.imageTag),
      essential: true,
      logging: ecs.LogDrivers.awsLogs({ logGroup, streamPrefix: 'probe' }),
    });

    const subnetIds = vpc.selectSubnets({ subnetType: ec2.SubnetType.PUBLIC }).subnetIds;

    const outputs: Record<string, string> = {
      ClusterName: cluster.clusterName,
      TaskDefinitionArn: taskDefinition.taskDefinitionArn,
      RepositoryUri: repository.repositoryUri,
      LogGroupName: logGroup.logGroupName,
      SecurityGroupId: securityGroup.securityGroupId,
      SubnetIds: cdk.Fn.join(',', subnetIds),
    };
    for (const [key, value] of Object.entries(outputs)) {
      new cdk.CfnOutput(this, key, { value });
    }
  }

  /** Public subnets only, so the task reaches ECR and Logs without a NAT gateway. */
  private vpc(vpcId?: string): ec2.IVpc {
    if (vpcId) {
      return ec2.Vpc.fromLookup(this, 'Vpc', { vpcId });
    }

    return new ec2.Vpc(this, 'Vpc', {
      vpcName: NAME_PREFIX,
      ipAddresses: ec2.IpAddresses.cidr(VPC_CIDR),
      maxAzs: MAX_AZS,
      natGateways: 0,
      subnetConfiguration: [{ name: 'public', subnetType: ec2.SubnetType.PUBLIC, cidrMask: 26 }],
      restrictDefaultSecurityGroup: false,
    });
  }
}
