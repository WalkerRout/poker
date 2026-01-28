# counter

Interface to track a (custom) limited count.

Currently serving using CloudFlare nameservers and a `cloudflared` tunnel with a Route53 domain...

TODO: Mark completed games as immutable, just in case other people accidentally change the contents
- Backup database every midnight, so we can cross reference to determine foul play...