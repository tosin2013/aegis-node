// Command approver-bot is a minimal mTLS approver client used by the
// cross-language conformance harness for the Rust MtlsApprovalChannel
// (issue #36). It is intentionally narrow:
//
//  1. Loads the supplied client cert + key.
//  2. Dials the addr with TLS, presenting that cert and skipping
//     server-cert validation (the test counterpart's design — server
//     cert authentication is the mTLS channel's separate concern).
//  3. Reads one line: the JSON-encoded approval request.
//  4. Writes one line: the granted/rejected decision JSON.
//
// Exits 0 on success. Any I/O / TLS error → non-zero exit.
package main

import (
	"bufio"
	"crypto/tls"
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"os"
)

type decision struct {
	Decision string `json:"decision"`
	Reason   string `json:"reason,omitempty"`
}

func main() {
	addr := flag.String("addr", "", "host:port of the mTLS server")
	certPath := flag.String("client-cert", "", "client cert PEM path")
	keyPath := flag.String("client-key", "", "client key PEM path")
	decisionStr := flag.String("decision", "granted", "granted | rejected")
	reason := flag.String("reason", "out of policy", "reason when --decision=rejected")
	flag.Parse()

	if *addr == "" || *certPath == "" || *keyPath == "" {
		log.Fatal("--addr, --client-cert, --client-key are required")
	}

	cert, err := tls.LoadX509KeyPair(*certPath, *keyPath)
	if err != nil {
		log.Fatalf("load client cert/key: %v", err)
	}

	// InsecureSkipVerify mirrors the Rust test's AcceptAnyServerCert —
	// the channel under test authenticates the *client*; cross-host
	// server-cert authentication is a separate concern that we do not
	// exercise here.
	cfg := &tls.Config{
		Certificates:       []tls.Certificate{cert},
		InsecureSkipVerify: true, //nolint:gosec // documented above
		MinVersion:         tls.VersionTLS12,
	}

	conn, err := tls.Dial("tcp", *addr, cfg)
	if err != nil {
		log.Fatalf("tls dial %s: %v", *addr, err)
	}
	defer conn.Close()

	reader := bufio.NewReader(conn)
	requestLine, err := reader.ReadString('\n')
	if err != nil {
		log.Fatalf("read request line: %v", err)
	}
	fmt.Fprintf(os.Stderr, "request: %s", requestLine)

	d := decision{Decision: *decisionStr}
	if *decisionStr == "rejected" {
		d.Reason = *reason
	}
	body, err := json.Marshal(d)
	if err != nil {
		log.Fatalf("marshal decision: %v", err)
	}
	body = append(body, '\n')
	if _, err := conn.Write(body); err != nil {
		log.Fatalf("write decision: %v", err)
	}
}
