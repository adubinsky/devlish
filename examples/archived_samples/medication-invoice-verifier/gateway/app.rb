# frozen_string_literal: true

require 'sinatra/base'
require 'json'
require_relative 'invoice_verification_job'

class MedicationInvoiceGateway < Sinatra::Base
  configure do
    set :show_exceptions, false
  end

  before do
    content_type :json
  end

  get '/health' do
    { status: 'ok', service: 'medication-invoice-gateway' }.to_json
  end

  # Event ingress endpoint.
  # Expected body: invoice payload JSON from upstream prescription/invoice system.
  post '/events/medication_invoice_submitted' do
    body = request.body.read
    payload = body.empty? ? {} : JSON.parse(body)

    required = %w[patient_id medication_name ndc_code quantity refill_count service_date invoice_id]
    missing = required.reject { |key| payload.key?(key) }

    if missing.any?
      status 422
      return({ status: 'error', error: 'missing_fields', missing: missing }.to_json)
    end

    # Queue async verification request; worker writes decision payload.
    jid = InvoiceVerificationJob.perform_async(payload)

    status 202
    { status: 'queued', job_id: jid, invoice_id: payload['invoice_id'] }.to_json
  rescue JSON::ParserError
    status 400
    { status: 'error', error: 'invalid_json' }.to_json
  end

  error do
    status 500
    { status: 'error', error: env['sinatra.error']&.message || 'unknown_error' }.to_json
  end
end
