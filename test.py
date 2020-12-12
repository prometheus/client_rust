import prometheus_client.openmetrics.parser

families = prometheus_client.openmetrics.parser.text_string_to_metric_families("test 123 1332 123  123 123 ")
list(families)
